use std::{env, path::PathBuf};

fn main() {
    let artifacts_dir = PathBuf::from("flutter_artifacts");
    let include_dir = artifacts_dir.join("include");
    let lib_dir = artifacts_dir.join("lib");

    // sanity checks
    assert!(artifacts_dir.is_dir(), "flutter_artifacts/ not found");
    assert!(include_dir.is_dir(), "flutter_artifacts/include/ not found");
    assert!(lib_dir.is_dir(), "flutter_artifacts/lib/ not found");
    assert!(
        include_dir.join("flutter_windows.h").is_file(),
        "flutter_windows.h missing"
    );
    assert!(
        include_dir.join("flutter_embedder.h").is_file(),
        "flutter_embedder.h missing"
    );
    assert!(
        lib_dir.join("flutter_windows.lib").is_file(),
        "flutter_windows.lib missing"
    );
    assert!(
        lib_dir.join("flutter_engine.lib").is_file(),
        "flutter_engine.lib missing"
    );

    // tell cargo where to find libs
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=flutter_windows");
    println!("cargo:rustc-link-lib=dylib=flutter_engine");

    // rerun if headers change
    println!(
        "cargo:rerun-if-changed={}",
        include_dir.join("flutter_windows.h").display()
    );
    println!("cargo:rerun-if-changed={}", include_dir.display());

    let binding = include_dir.join("flutter_windows.h");
    let header_path = binding.to_str().expect("Invalid include path");
    let bindings = bindgen::Builder::default()
        .header(header_path)
        .clang_arg(format!("-I{}", include_dir.display()))
        .allowlist_function("Flutter.*")
        .allowlist_type("Flutter.*")
        .allowlist_var("Flutter.*")
        .allowlist_var("kFlutter.*")
        .layout_tests(false)
        .derive_default(true)
        .generate()
        .expect("Unable to generate bindings");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out.join("bindings.rs"))
        .expect("couldn’t write bindings");
}
