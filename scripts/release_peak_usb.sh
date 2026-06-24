#!/usr/bin/env bash
# Release PEAK PCAN-USB FD adapters from the in-kernel `peak_usb` driver so a browser
# (WebUSB) or raw libusb can claim them.
#
# Re-bind later by replugging the device, or:
#   sudo modprobe -r peak_usb && sudo modprobe peak_usb
set -euo pipefail

DRIVER=/sys/bus/usb/drivers/peak_usb

if [ ! -d "$DRIVER" ]; then
    echo "peak_usb driver not loaded; nothing bound."
    exit 0
fi

found=0
for iface in "$DRIVER"/*:*; do
    [ -e "$iface" ] || continue
    name=$(basename "$iface")
    echo "Unbinding $name from peak_usb"
    echo -n "$name" | sudo tee "$DRIVER/unbind" > /dev/null
    found=1
done

if [ "$found" -eq 0 ]; then
    echo "No PEAK devices currently bound to peak_usb."
fi
