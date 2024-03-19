#!/usr/bin/env python3
import argparse

from scapy.all import *


def forwarding1(pkt):
    return pkt

def forwarding2(pkt):
    return False


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--iface", type=str, default="vcan0")
    parser.add_argument("--rx", type=int, default=0x7a1)
    parser.add_argument("--tx", type=int, default=0x7a9)
    parser.add_argument("--timeout", type=int, default=10)
    parser.add_argument("--kernel-isotp", type=bool, default=True)
    parser.add_argument("--stmin", type=int, default=0)
    parser.add_argument("--bs", type=int, default=0)
    parser.add_argument("--padding", type=int, nargs="?", default=None, const=0xaa)
    parser.add_argument("--fd", action=argparse.BooleanOptionalAction)

    args = parser.parse_args()

    conf.contribs['ISOTP'] = {'use-can-isotp-kernel-module': args.kernel_isotp}
    load_contrib('isotp')
    load_contrib('automotive.uds')
    load_contrib('automotive.ecu')

    config = {
        'stmin': args.stmin,
        'padding': args.padding,
        'bs': args.bs,
        'fd': args.fd,
    }

    with ISOTPSocket(args.iface, tx_id=args.tx, rx_id=args.rx, **config) as sock1:
        with ISOTPSocket(args.iface, tx_id=args.tx, rx_id=args.rx, listen_only=True, **config) as sock2:
            sock1.send(b'\xAA') # Signal to test that ECU is ready
            bridge_and_sniff(if1=sock1, if2=sock2, xfrm12=forwarding1, xfrm21=forwarding2, timeout=args.timeout)
