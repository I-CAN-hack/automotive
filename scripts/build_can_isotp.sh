#!/usr/bin/env sh
set -e

cd /tmp
git clone https://github.com/hartkopp/can-isotp.git
cd can-isotp
make
sudo make modules_install
