//! Minimal WebUSB demo: connect to a panda or a PEAK PCAN-USB FD over WebUSB and print
//! incoming CAN data.
//!
//! Build/run with [Trunk](https://trunkrs.dev): `trunk serve` from this directory, then
//! open the page in a Chromium-based browser (WebUSB is not supported in Firefox/Safari).

use automotive::can::bitrate::{BitrateBuilder, BitrateConfig};
use automotive::can::AsyncCanAdapter;
use automotive::panda::{Panda, PANDA_TIMING_CONST};
use automotive::peak::{Peak, PEAK_TIMING_CONST};
use automotive::StreamExt;
use std::future::Future;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::Document;

fn main() {
    console_error_panic_hook::set_once();

    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");

    // WebUSB's requestDevice must run from a user gesture, so each button kicks off its
    // connection from the click handler.
    wire_button(&document, "connect-panda", || {
        Panda::connect_async(bitrate(PANDA_TIMING_CONST))
    });
    wire_button(&document, "connect-peak", || {
        Peak::connect_async(bitrate(PEAK_TIMING_CONST))
    });

    log("Ready. Pick an adapter to connect.");
}

/// Wire a button to a function that connects to an adapter and returns an [`AsyncCanAdapter`].
fn wire_button<F, Fut>(document: &Document, id: &str, connect: F)
where
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = automotive::Result<AsyncCanAdapter>> + 'static,
{
    let button = document
        .get_element_by_id(id)
        .unwrap_or_else(|| panic!("missing #{id} button"));

    let onclick = Closure::<dyn FnMut()>::new(move || {
        let connecting = connect();
        spawn_local(async move {
            match connecting.await {
                Ok(adapter) => stream_frames(adapter).await,
                Err(e) => log(&format!("error: {e}")),
            }
        });
    });
    button
        .add_event_listener_with_callback("click", onclick.as_ref().unchecked_ref())
        .expect("failed to add click listener");
    onclick.forget();
}

/// Stream frames from an adapter to the page until the stream ends.
async fn stream_frames(adapter: AsyncCanAdapter) {
    log("Connected. Streaming CAN frames…");

    // `adapter` is kept alive for the duration of the stream; the background processing
    // task is stopped when it is dropped.
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
}

fn bitrate(timing: automotive::can::bitrate::AdapterTimingConst) -> BitrateConfig {
    BitrateBuilder::with_timing_const(timing)
        .bitrate(500_000)
        .sample_point(0.8)
        .data_bitrate(2_000_000)
        .data_sample_point(0.8)
        .build()
        .expect("invalid bitrate config")
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
