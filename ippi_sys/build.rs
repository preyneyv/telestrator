use std::path::Path;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn main() {
    println!("cargo::rerun-if-changed=build.rs");

    let sdk_path = Path::new(ROOT).join("sdk");

    println!(
        "cargo::rustc-link-search={}",
        sdk_path.join("lib").to_string_lossy()
    );
    println!("cargo::rustc-link-lib=ippcc");
    println!("cargo::rustc-link-lib=ippcore");

    bindgen::builder()
        .raw_line("#![allow(dead_code)]")
        .raw_line("#![allow(non_camel_case_types)]")
        .raw_line("#![allow(non_snake_case)]")
        .raw_line("#![allow(non_upper_case_globals)]")
        .raw_line("#![allow(unused_imports)]")
        .clang_arg(format!("-I{}", sdk_path.join("include").to_string_lossy()))
        .header(sdk_path.join("include").join("ipp.h").to_string_lossy())
        .default_enum_style(bindgen::EnumVariation::ModuleConsts)
        .derive_partialeq(true)
        .trust_clang_mangling(false)
        .generate()
        .unwrap()
        .write_to_file(format!("{ROOT}/src/ffi.rs"))
        .unwrap();
}
