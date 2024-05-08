use std::env;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=IPPROOT");

    // create bindings and link Intel Performance Primitives, if present.
    if let Ok(ipproot) = env::var("IPPROOT") {
        let bindings = bindgen::builder()
            .header(format!("{}/include/ipp.h", ipproot))
            .default_enum_style(bindgen::EnumVariation::ModuleConsts)
            .derive_partialeq(true)
            .trust_clang_mangling(false)
            .raw_line("#![allow(dead_code)]")
            .raw_line("#![allow(non_camel_case_types)]")
            .raw_line("#![allow(non_snake_case)]")
            .raw_line("#![allow(non_upper_case_globals)]")
            .generate()
            .unwrap();

        bindings.write_to_file("src/ffi/ippi.rs").unwrap();
        println!(
            "cargo::rustc-link-arg=/LIBPATH:{}",
            env::var_os("IPPROOT").unwrap().to_string_lossy()
        );
        println!("cargo::rustc-link-arg=ippcc.lib");
        println!("cargo::rustc-link-arg=ippcore.lib");
    }
}
