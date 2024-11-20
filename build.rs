use copy_to_output::copy_to_output;
use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-link-search=third_party/vector");
    println!("cargo:rustc-link-lib=vxlapi64");

    let bindings = bindgen::Builder::default()
        .header("third_party/vector/wrapper.h")
        .clang_arg("-Wno-pragma-pack")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("vxlapi_bindings.rs"))
        .expect("Couldn't write bindings!");

    copy_to_output(
        "third_party/vector/vxlapi64.dll",
        &env::var("PROFILE").unwrap(),
    )
    .expect("Could not copy DLL");
}
