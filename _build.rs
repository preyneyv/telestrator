// use std::{env, path::Path};

// const ROOT: &str = env!("CARGO_MANIFEST_DIR");

// fn shared_builder() -> bindgen::Builder {
//     bindgen::builder()
//         .raw_line("#![allow(dead_code)]")
//         .raw_line("#![allow(non_camel_case_types)]")
//         .raw_line("#![allow(non_snake_case)]")
//         .raw_line("#![allow(non_upper_case_globals)]")
//         .raw_line("#![allow(unused_imports)]")
// }

// fn lib_path(lib_name: &str) -> String {
//     let path = format!("{ROOT}/lib/{lib_name}");
//     println!("cargo::rerun-if-changed={path}");
//     path
// }

// fn bind_ippi() {
//     let lib = lib_path("ippi");
//     let out = Path::new("src/ffi/ippi.rs");

//     println!("cargo::rustc-link-arg=/LIBPATH:{lib}/lib");
//     println!("cargo::rustc-link-arg=ippcc.lib");
//     println!("cargo::rustc-link-arg=ippcore.lib");

//     if !out.exists() {
//         shared_builder()
//             .clang_arg(format!("-I{lib}/include"))
//             .header(format!("{lib}/include/ipp.h"))
//             .default_enum_style(bindgen::EnumVariation::ModuleConsts)
//             .derive_partialeq(true)
//             .trust_clang_mangling(false)
//             .generate()
//             .unwrap()
//             .write_to_file(out)
//             .unwrap();
//     }
// }

// fn bind_nvidia_video_codec() {
//     let lib = lib_path("nvidia_video_codec");
//     let out = Path::new("src/feed/encoders/nvenc/ffi.rs");

//     println!("cargo::rustc-link-arg=/LIBPATH:{lib}/Lib/Win32");

//     if !out.exists() {
//         shared_builder()
//             .clang_arg(format!("-I{lib}/Interface"))
//             .header(format!("{lib}/Interface/nvEncodeAPI.h"))
//             .generate()
//             .unwrap()
//             .write_to_file(out)
//             .unwrap();
//     }
// }

// fn main() {
//     println!("cargo::rerun-if-changed=build.rs");

//     // bind_ippi();
//     // bind_nvidia_video_codec();
// }
