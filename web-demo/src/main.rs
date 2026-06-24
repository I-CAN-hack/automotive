//! Minimal WebUSB demo: connect to the first available panda and print incoming CAN data.
//!
//! Build/run with [Trunk](https://trunkrs.dev): `trunk serve` from this directory, then
//! open the page in a Chromium-based browser (WebUSB is not supported in Firefox/Safari).

use automotive::can::bitrate::BitrateBuilder;
use automotive::panda::{Panda, PANDA_TIMING_CONST};
use automotive::StreamExt;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

fn main() {
    console_error_panic_hook::set_once();

    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");
    let button = document
        .get_element_by_id("connect")
        .expect("missing #connect button");

    // WebUSB's requestDevice must run from a user gesture, so kick off the work from the
    // click handler.
    let onclick = Closure::<dyn FnMut()>::new(move || {
        wasm_bindgen_futures::spawn_local(async {
            if let Err(e) = run().await {
                log(&format!("error: {e}"));
            }
        });
    });
    button
        .add_event_listener_with_callback("click", onclick.as_ref().unchecked_ref())
        .expect("failed to add click listener");
    onclick.forget();

    log("Ready. Click \"Connect to panda\" and pick your device.");
}

async fn run() -> automotive::Result<()> {
    let bitrate = BitrateBuilder::with_timing_const(PANDA_TIMING_CONST)
        .bitrate(500_000)
        .sample_point(0.8)
        .data_bitrate(2_000_000)
        .data_sample_point(0.8)
        .build()
        .expect("invalid bitrate config");

    log("Requesting panda over WebUSB…");
    let adapter = Panda::connect_async(bitrate).await?;
    log("Connected. Streaming CAN frames…");

    // `adapter` (the AsyncCanAdapter) must stay alive for the duration of the stream; the
    // background processing task is stopped when it is dropped.
    let mut stream = adapter.recv();
    while let Some(frame) = stream.next().await {
        let id: u32 = frame.id.into();
        log(&format!(
            "bus {}  id 0x{:x}  [{}]  {}",
            frame.bus,
            id,
            frame.data.len(),
            hex::encode(&frame.data)
        ));
    }

    log("Stream ended.");
    Ok(())
}

/// Append a line to the page's `#output` element and mirror it to the dev console.
fn log(msg: &str) {
    web_sys::console::log_1(&JsValue::from_str(msg));
    if let Some(output) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("output"))
    {
        let prev = output.text_content().unwrap_or_default();
        output.set_text_content(Some(&format!("{prev}{msg}\n")));
    }
}
