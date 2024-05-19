use bytes::BufMut;
use webrtc::{
    rtp::{extension::HeaderExtension, Error},
    util::{Marshal, MarshalSize, Unmarshal},
};

pub const PLAYOUT_DELAY_EXTENSION_SIZE: usize = 3;

/// An RTP extension to provide the receiver a hint for jitter buffer delay.
/// http://www.webrtc.org/experiments/rtp-hdrext/playout-delay
pub struct PlayoutDelayExtension {
    pub min_delay: u16,
    pub max_delay: u16,
}

impl Unmarshal for PlayoutDelayExtension {
    fn unmarshal<B>(buf: &mut B) -> webrtc::util::Result<Self>
    where
        Self: Sized,
        B: bytes::Buf,
    {
        if buf.remaining() < PLAYOUT_DELAY_EXTENSION_SIZE {
            return Err(Error::ErrBufferTooSmall.into());
        }

        let b0 = buf.get_u8();
        let b1 = buf.get_u8();
        let b2 = buf.get_u8();

        let min_delay = u16::from_be_bytes([b0, b1]) >> 4;
        let max_delay = u16::from_be_bytes([b1, b2]) & 0x0FFF;

        Ok(PlayoutDelayExtension {
            min_delay,
            max_delay,
        })
    }
}

impl MarshalSize for PlayoutDelayExtension {
    fn marshal_size(&self) -> usize {
        PLAYOUT_DELAY_EXTENSION_SIZE
    }
}

impl Marshal for PlayoutDelayExtension {
    fn marshal_to(&self, mut buf: &mut [u8]) -> webrtc::util::Result<usize> {
        if buf.remaining_mut() < PLAYOUT_DELAY_EXTENSION_SIZE {
            return Err(Error::ErrBufferTooSmall.into());
        }

        buf.put_u8((self.min_delay >> 4) as u8);
        buf.put_u8(((self.min_delay << 4) as u8) | (self.max_delay >> 8) as u8);
        buf.put_u8(self.max_delay as u8);

        return Ok(PLAYOUT_DELAY_EXTENSION_SIZE);
    }
}

impl PlayoutDelayExtension {
    pub fn new(min_delay: u16, max_delay: u16) -> Self {
        PlayoutDelayExtension {
            min_delay,
            max_delay,
        }
    }

    pub fn to_extension(self) -> HeaderExtension {
        HeaderExtension::Custom {
            uri: "http://www.webrtc.org/experiments/rtp-hdrext/playout-delay".into(),
            extension: Box::from(self),
        }
    }
}
