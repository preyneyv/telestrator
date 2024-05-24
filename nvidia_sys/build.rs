use std::{fs, path::Path};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn make_builder() -> bindgen::Builder {
    bindgen::builder()
        .raw_line("#![allow(dead_code)]")
        .raw_line("#![allow(non_camel_case_types)]")
        .raw_line("#![allow(non_snake_case)]")
        .raw_line("#![allow(non_upper_case_globals)]")
        .raw_line("#![allow(unused_imports)]")
        // .default_enum_style(bindgen::EnumVariation::ModuleConsts)
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: true,
        })
        .derive_partialeq(true)
        .derive_default(true)
    // .trust_clang_mangling(false)
    // .clang_arg(format!("-I{sdk_path}/Interface"))
    // .header(format!("{sdk_path}/Interface/{header}"))
}

fn write(builder: bindgen::Builder, path: &str) {
    builder
        .generate()
        .unwrap()
        .write_to_file(format!("{ROOT}/src/ffi/{path}"))
        .unwrap();
}

fn main() {
    fs::create_dir_all(format!("{ROOT}/src/ffi")).unwrap();
    println!("cargo::rerun-if-changed=build.rs");

    let cuda_sdk = Path::new(ROOT).join("sdk").join("cuda");
    let cuda_include = cuda_sdk.join("include");
    let cuda_cl_arg = format!("-I{}", cuda_include.to_string_lossy());
    println!(
        "cargo::rustc-link-search={}",
        cuda_sdk.join("lib").join("x64").to_string_lossy()
    );
    println!("cargo::rustc-link-lib=cuda");

    let nvc_sdk = Path::new(ROOT).join("sdk").join("nvidia_video_codec");
    let nvc_include = nvc_sdk.join("Interface");
    let nvc_cl_arg = format!("-I{}", nvc_include.to_string_lossy());
    println!(
        "cargo::rustc-link-search={}",
        nvc_sdk.join("Lib").join("x64").to_string_lossy()
    );
    println!("cargo::rustc-link-lib=nvencodeapi");
    println!("cargo::rustc-link-lib=nvcuvid");

    write(
        make_builder()
            .clang_arg(&cuda_cl_arg)
            .header(cuda_include.join("cuda.h").to_string_lossy()),
        "cuda.rs",
    );

    write(
        make_builder()
            .clang_arg(&nvc_cl_arg)
            .clang_arg(&cuda_cl_arg)
            .header(nvc_include.join("nvcuvid.h").to_string_lossy())
            .allowlist_file(".*[/\\\\]nvcuvid.h"),
        "nvcuvid.rs",
    );

    write(
        make_builder()
            .clang_arg(&nvc_cl_arg)
            .clang_arg(&cuda_cl_arg)
            .header(nvc_include.join("cuviddec.h").to_string_lossy())
            .allowlist_file(".*[/\\\\]cuviddec.h"),
        "cuviddec.rs",
    );

    write(
        make_builder()
            .clang_arg(&nvc_cl_arg)
            .clang_arg(&cuda_cl_arg)
            .header("src/wrappers/nvenc.h")
            .allowlist_file(".*[/\\\\]nvenc.h")
            .header(nvc_include.join("nvEncodeAPI.h").to_string_lossy())
            .allowlist_file(".*[/\\\\]nvEncodeAPI.h"),
        "nvencodeapi.rs",
    );
}
