# automotive — WebUSB CAN demo

A minimal browser app that connects to the first available comma.ai panda over **WebUSB**
and prints incoming CAN frames. It uses the same `automotive::can::AsyncCanAdapter` API as
the native adapters — the only difference is the panda is connected with
`Panda::connect_async(...)` (WebUSB) instead of `Panda::new_async(...)` (libusb).

## Requirements

- A Chromium-based browser (Chrome/Edge). **WebUSB is not supported in Firefox or Safari.**
- Served over a secure context — `localhost` (which `trunk serve` uses) or HTTPS.
- [Trunk](https://trunkrs.dev): `cargo install trunk`
- The wasm target: `rustup target add wasm32-unknown-unknown`

## Run

From this directory:

```sh
trunk serve --open
```

Then click **Connect to panda** and pick your device from the browser's chooser. Incoming
CAN frames are printed to the page and the dev console.

On Linux you may need a udev rule (or to run the browser with sufficient permissions) for
WebUSB to access the panda, e.g. allow the panda's USB vendor IDs (`0xbbaa`, `0x3801`).

## Notes

- `--cfg=web_sys_unstable_apis` is required for web-sys' WebUSB bindings; it is set
  automatically via `.cargo/config.toml`.
- This crate is standalone (its own `[workspace]`) so it is not built as part of the parent
  `automotive` crate's native build.
