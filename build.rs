#[cfg(all(target_os = "windows", feature = "vector-xl"))]
fn build_vxlapi() {
    use std::env;
    use std::path::{Path, PathBuf};

    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    println!(
        "cargo:rustc-link-search={}",
        Path::new(&dir).join("third_party/vector").display()
    );
    println!("cargo:rustc-link-lib=vxlapi64");

    let bindings = bindgen::Builder::default()
        .header(format!(
            "{}",
            Path::new(&dir)
                .join("third_party/vector/wrapper.h")
                .display()
        ))
        .clang_arg("-Wno-pragma-pack")
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("vxlapi_bindings.rs"))
        .expect("Couldn't write bindings!");
}

fn main() {
    #[cfg(all(target_os = "windows", feature = "vector-xl"))]
    build_vxlapi();
}
