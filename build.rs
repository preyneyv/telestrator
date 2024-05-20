use std::env;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn make_builder() -> bindgen::Builder {
    bindgen::builder()
        .raw_line("#![allow(dead_code)]")
        .raw_line("#![allow(non_camel_case_types)]")
        .raw_line("#![allow(non_snake_case)]")
        .raw_line("#![allow(non_upper_case_globals)]")
}

fn bind_ippi() {
    println!("cargo::rustc-link-arg=/LIBPATH:{ROOT}/lib/ippi/lib");
    println!("cargo::rustc-link-arg=ippcc.lib");
    println!("cargo::rustc-link-arg=ippcore.lib");

    let bindings = make_builder()
        .header("lib/ippi/include/ipp.h")
        .default_enum_style(bindgen::EnumVariation::ModuleConsts)
        .derive_partialeq(true)
        .trust_clang_mangling(false)
        .generate()
        .unwrap();

    bindings.write_to_file("src/ffi/ippi.rs").unwrap();
}

fn bind_nvidia_video_codec() {
    println!("cargo::rustc-link-arg=/LIBPATH:{ROOT}/lib/nvidia_video_codec/Lib/Win32",);

    let bindings = bindgen::builder()
        .header("lib/nvidia_video_codec/Interface/nvEncodeAPI.h")
        .generate()
        .unwrap();
    bindings
        .write_to_file("src/feed/encoders/nvenc/ffi.rs")
        .unwrap();
}

fn main() {
    println!("cargo::rerun-if-changed=build.rs");

    bind_ippi();
    bind_nvidia_video_codec();
}
