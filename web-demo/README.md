# automotive — WebUSB CAN demo

A minimal browser app that connects to a CAN adapter over **WebUSB** and prints incoming
CAN frames. It supports the comma.ai **panda** and the **PEAK PCAN-USB FD** family, using
the same `automotive::can::AsyncCanAdapter` API as the native adapters — the only
difference is the adapter is connected with `connect_async(...)` (WebUSB) instead of
`new_async(...)` (libusb).

## PEAK on Linux: unbind the kernel driver first

WebUSB cannot detach an in-kernel driver. On Linux the PCAN-USB FD is claimed by the
`peak_usb` module, so the browser's `claimInterface` will fail until you unbind it:

```sh
# Find the device's USB path, then unbind it from peak_usb:
echo -n '1-2:1.0' | sudo tee /sys/bus/usb/drivers/peak_usb/unbind
```

(Replace `1-2:1.0` with your device's `<bus>-<port>:1.0` from `ls /sys/bus/usb/drivers/peak_usb/`.)
Alternatively blocklist `peak_usb`. The panda has no kernel driver and needs none of this.

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
