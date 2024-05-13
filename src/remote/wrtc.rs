/// Differences:
/// -[ ] retransmission (rtx)
/// -[ ] reduce keyframe count (use fir and pli)
/// -[ ] bonus points: use bwe to pick bitrate
/// -[ ] set duration accurately
///
/// alternative
/// -[ ] disable frameskip on encoder (not recommended, blows up max bitrate )
use std::{convert::Infallible, net::SocketAddr, str::FromStr, sync::Arc, time::Instant};

use anyhow::Result;
use static_dir::static_dir;
use tokio::{
    sync::{broadcast, mpsc, oneshot, Notify},
    task::JoinHandle,
    try_join,
};
use uuid::Uuid;
use warp::Filter;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_H264},
        APIBuilder,
    },
    ice_transport::ice_connection_state::RTCIceConnectionState,
    interceptor::registry::Registry,
    media::Sample,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::{rtp_codec::RTCRtpCodecCapability, RTCPFeedback, TYPE_RTCP_FB_CCM},
    track::track_local::{track_local_static_sample::TrackLocalStaticSample, TrackLocal},
};

use crate::feed::manager::{FeedControlMessage, FeedResultMessage};

#[derive(Debug)]
pub struct WrtcOffer {
    sdp: RTCSessionDescription,
    resp: oneshot::Sender<Option<RTCSessionDescription>>,
}

async fn handle_new_offer(
    sdp: RTCSessionDescription,
    offer_tx: mpsc::Sender<WrtcOffer>,
) -> std::result::Result<impl warp::Reply, Infallible> {
    let (resp_tx, resp_rx) = oneshot::channel();
    offer_tx
        .send(WrtcOffer { sdp, resp: resp_tx })
        .await
        .unwrap();
    let reply = resp_rx.await.unwrap_or(None);
    Ok(warp::reply::json(&reply))
}

fn signalling_server(port: u16) -> (mpsc::Receiver<WrtcOffer>, JoinHandle<()>) {
    let (offer_tx, offer_rx) = mpsc::channel::<WrtcOffer>(1);

    let offer_handler = warp::post()
        .and(warp::path!("wrtc" / "offer"))
        .and(warp::body::json())
        .and(warp::any().map(move || offer_tx.clone()))
        .and_then(handle_new_offer);

    let static_handler = warp::get().and(static_dir!("./www"));

    let cors = warp::cors()
        .allow_any_origin()
        .allow_headers(vec![
            "User-Agent",
            "Sec-Fetch-Mode",
            "Referer",
            "Origin",
            "Access-Control-Request-Method",
            "Access-Control-Request-Headers",
            "Content-Type",
        ])
        .allow_methods(&[
            warp::hyper::Method::PUT,
            warp::hyper::Method::DELETE,
            warp::hyper::Method::POST,
            warp::hyper::Method::GET,
        ])
        .build();

    let server = warp::serve(offer_handler.or(static_handler).with(cors));
    let addr = SocketAddr::from_str(&format!("0.0.0.0:{port}")).unwrap();
    let task = tokio::task::spawn(server.run(addr));
    println!("Remote listening on http://0.0.0.0:{port}");
    (offer_rx, task)
}

async fn webrtc_worker(
    offer: WrtcOffer,
    mut feed_result_rx: broadcast::Receiver<FeedResultMessage>,
    feed_control_tx: mpsc::Sender<FeedControlMessage>,
) -> Result<()> {
    let client_id = Uuid::new_v4().to_string();
    // Setup webrtc internals
    // TODO: see what can be moved out.
    let mut m = MediaEngine::default();
    m.register_default_codecs().unwrap();

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut m)?;

    let api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();

    let config = RTCConfiguration {
        ..Default::default()
    };
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    let notify_tx = Arc::new(Notify::new());
    let (done_tx, mut done_rx) = mpsc::channel::<()>(1);

    let video_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            clock_rate: 90000,
            channels: 0,
            // sdp_fmtp_line: "".into(),
            // rtcp_feedback: vec![RTCPFeedback {
            //     typ: TYPE_RTCP_FB_CCM.into(),
            //     parameter: "fir".into(),
            // }],
            ..Default::default()
        },
        "video".to_owned(),
        "telestrator".to_owned(),
    ));

    let rtp_sender = peer_connection
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    // Read incoming RTCP
    tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
        Result::<()>::Ok(())
    });

    let notify_video = notify_tx.clone();
    let video_done_tx = done_tx.clone();
    let video_feed_ctrl_tx = feed_control_tx.clone();
    let video_client_id = client_id.clone();

    let video_task = tokio::spawn(async move {
        notify_video.notified().await;
        println!("ready to send video");
        video_feed_ctrl_tx
            .send(FeedControlMessage::ClientJoined {
                client_id: video_client_id,
            })
            .await?;

        let mut last_write = Instant::now();

        loop {
            let data = match feed_result_rx.recv().await {
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,

                Ok(FeedResultMessage::EncodedBitstream(data)) => data,
            };

            let now = Instant::now();
            video_track
                .write_sample(&Sample {
                    data,
                    duration: now - last_write,
                    ..Default::default()
                })
                .await?;

            last_write = now;
        }

        video_done_tx.try_send(()).ok();

        Result::<()>::Ok(())
    });

    peer_connection.on_ice_connection_state_change(Box::new(
        move |connection_state: RTCIceConnectionState| {
            println!("Connection State has changed {connection_state}");
            if connection_state == RTCIceConnectionState::Connected {
                notify_tx.notify_waiters();
            }
            Box::pin(async {})
        },
    ));

    let peer_connection_failed_tx = done_tx.clone();
    peer_connection.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        if s == RTCPeerConnectionState::Failed {
            println!("Peer Connection has gone to failed exiting");
            peer_connection_failed_tx.try_send(()).ok();
        }
        Box::pin(async {})
    }));

    // Present connection
    let sdp = offer.sdp;
    peer_connection.set_remote_description(sdp).await?;
    let answer = peer_connection.create_answer(None).await?;

    peer_connection.set_local_description(answer).await?;

    peer_connection
        .gathering_complete_promise()
        .await
        .recv()
        .await;

    let Some(local_description) = peer_connection.local_description().await else {
        return Ok(());
    };
    offer.resp.send(Some(local_description)).unwrap();

    tokio::select! {
        _ = done_rx.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }

    video_task.abort();
    feed_control_tx
        .send(FeedControlMessage::ClientLeft { client_id })
        .await?;
    println!("goodbye thread");

    Ok(())
}

pub async fn run_webrtc_tasks(
    feed_control_tx: mpsc::Sender<FeedControlMessage>,
    frame_ready_tx: broadcast::Sender<FeedResultMessage>,
) -> Result<()> {
    let (mut sdp_rx, http_task) = signalling_server(8888);

    let wrtc_manager = tokio::task::spawn(async move {
        while let Some(offer) = sdp_rx.recv().await {
            tokio::task::spawn(webrtc_worker(
                offer,
                frame_ready_tx.subscribe(),
                feed_control_tx.clone(),
            ));
        }
    });

    try_join!(wrtc_manager, http_task)?;

    Ok(())
}
