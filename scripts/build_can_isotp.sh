#!/usr/bin/env sh
set -e

sudo modprobe can

if sudo modprobe can-isotp 2>/dev/null; then
  exit 0
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

git clone --depth 1 --branch mainline-6.17+ https://github.com/I-CAN-hack/can-isotp.git "$tmpdir/can-isotp"
cd "$tmpdir/can-isotp"
make

sudo insmod ./net/can/can-isotp.ko
