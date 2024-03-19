#!/usr/bin/env sh
set -e

sudo modprobe can
sudo modprobe can-isotp
sudo modprobe vcan
sudo ip link add dev vcan0 type vcan
sudo ip link set dev vcan0 up
