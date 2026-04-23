use std::env;

#[cfg(feature = "vector-xl")]
fn build_vxlapi() {
    use std::env;
    use std::path::{Path, PathBuf};

    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    const ALLOWED_ITEMS: &[&str] = &[
        "XL_BUS_.*",
        "XL_CAN_.*",
        "XL_ERR_.*",
        "XL_HWTYPE_.*",
        "XL_INTERFACE_VERSION_.*",
        "XL_SUCCESS",
        "xlActivateChannel",
        "xlCanFdSetConfiguration",
        "xlCanReceive",
        "xlCanTransmitEx",
        "xlCloseDriver",
        "xlClosePort",
        "xlDeactivateChannel",
        "xlGetApplConfig",
        "xlGetChannelIndex",
        "xlGetChannelMask",
        "xlGetDriverConfig",
        "xlOpenDriver",
        "xlOpenPort",
    ];

    let mut bindings = bindgen::Builder::default()
        .header(format!(
            "{}",
            Path::new(&dir)
                .join("third_party/vector/wrapper.h")
                .display()
        ))
        .clang_arg("-Wno-pragma-pack")
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .dynamic_library_name("Xl")
        .dynamic_link_require_all(true);

    for item in ALLOWED_ITEMS {
        bindings = bindings.allowlist_item(item);
    }

    let bindings = bindings.generate().expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("vxlapi_bindings.rs"))
        .expect("Couldn't write bindings!");
}

fn link_pcbusb() {
    // The MacCAN PCBUSB library ships as `libPCBUSB.dylib` in one of the
    // system-wide lib dirs. Hint the linker at both Intel (/usr/local) and
    // Apple Silicon (/opt/homebrew) prefixes.
    //
    // The dylib's install name is `@rpath/libPCBUSB.<ver>.dylib`, so we also
    // emit matching rpaths. `rustc-link-arg` applies to examples, tests, and
    // in-crate binaries — downstream consumers still need to add their own
    // rpath (mirroring the "distribute the DLL" model the Vector and J2534
    // adapters use on Windows).
    for dir in ["/usr/local/lib", "/opt/homebrew/lib"] {
        if std::path::Path::new(dir).exists() {
            println!("cargo:rustc-link-search=native={}", dir);
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir);
        }
    }
    println!("cargo:rustc-link-lib=dylib=PCBUSB");
}

fn main() {
    // Re-run if these change (helps with incremental builds / switching targets).
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-env-changed=HOST");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_VECTOR_XL");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_PCAN");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let vector_xl_enabled = env::var_os("CARGO_FEATURE_VECTOR_XL").is_some();
    let pcan_enabled = env::var_os("CARGO_FEATURE_PCAN").is_some();

    if target_os == "windows" && vector_xl_enabled {
        #[cfg(feature = "vector-xl")]
        build_vxlapi();
    }

    if target_os == "macos" && pcan_enabled {
        link_pcbusb();
    }
}
