#!/usr/bin/env sh
# Temporary Linux configuration for a directly connected DoIP ECU.
set -eu

usage() {
  printf 'Usage: %s [adapter [ipv4/prefix [ipv6/prefix]]]\n' "$0"
  printf 'Defaults: eth0 169.254.0.1/16 fe80::1/64\n'
  printf 'Choose tester addresses distinct from the ECU; changes last until reboot/reconfiguration.\n'
}

if [ "${1:-}" = --help ] || [ "${1:-}" = -h ]; then
  usage
  exit 0
fi
if [ "$#" -gt 3 ]; then
  usage >&2
  exit 2
fi
if [ "$(uname -s)" != Linux ]; then
  printf 'This script requires Linux and iproute2.\n' >&2
  exit 1
fi

adapter=${1:-eth0}
ipv4=${2:-169.254.0.1/16}
ipv6=${3:-fe80::1/64}

if [ "$(id -u)" -ne 0 ]; then
  exec sudo -- "$0" "$adapter" "$ipv4" "$ipv6"
fi

# Keep NetworkManager from removing the bench adapter's static addresses.
if command -v nmcli >/dev/null 2>&1 && nmcli general status >/dev/null 2>&1; then
  nmcli device set "$adapter" managed no
fi

# Enable IPv6 even if previously disabled, and use static addressing rather
# than router advertisements/SLAAC. Existing addresses and routes are retained.
sysctl -w "net/ipv6/conf/$adapter/disable_ipv6=0" \
  "net/ipv6/conf/$adapter/accept_ra=0" "net/ipv6/conf/$adapter/autoconf=0"
ip link set dev "$adapter" addrgenmode none
ip link set dev "$adapter" up
ip -4 address replace "$ipv4" broadcast + dev "$adapter"
ip -6 address replace "$ipv6" dev "$adapter"

# Broadcast/multicast discovery replies have unicast source addresses, so they
# do not match the outgoing UDP flow in a stateful firewall. Announcements use
# port 13400; active replies use the tester's dynamic UDP port.
for firewall in iptables ip6tables; do
  if command -v "$firewall" >/dev/null 2>&1; then
    if ! "$firewall" -C INPUT -i "$adapter" -p udp -m multiport --dports 13400,49152:65535 -j ACCEPT 2>/dev/null; then
      "$firewall" -I INPUT -i "$adapter" -p udp -m multiport --dports 13400,49152:65535 -j ACCEPT
    fi
  fi
done

# Wait for IPv6 duplicate-address detection before starting discovery.
attempt=0
while [ -n "$(ip -6 -o address show dev "$adapter" tentative)" ]; do
  if [ -n "$(ip -6 -o address show dev "$adapter" dadfailed)" ]; then
    break
  fi
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 10 ]; then
    printf 'IPv6 duplicate-address detection did not complete on %s.\n' "$adapter" >&2
    exit 1
  fi
  sleep 1
done
if [ -n "$(ip -6 -o address show dev "$adapter" dadfailed)" ]; then
  printf 'IPv6 address conflict on %s; choose another tester address.\n' "$adapter" >&2
  exit 1
fi

ip -brief address show dev "$adapter"
