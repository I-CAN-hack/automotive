#!/usr/bin/env sh
set -e

cd /tmp
git clone https://github.com/hartkopp/can-isotp.git
cd can-isotp
git checkout mainline-5.4+
make

sudo modprobe can
sudo insmod ./net/can/can-isotp.ko
