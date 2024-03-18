#!/usr/bin/env python3
import argparse
import threading
import time

from scapy.all import *
from scapy.ansmachine import AnsweringMachine


def forwarding1(pkt):
    return pkt

def forwarding2(pkt):
    return False


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--iface", type=str, default="vcan0")
    parser.add_argument("--rx", type=int, default=0x7a1)
    parser.add_argument("--tx", type=int, default=0x7a9)
    parser.add_argument("--protocol", type=str, default="iso-tp")
    parser.add_argument("--timeout", type=int, default=10)

    args = parser.parse_args()

    conf.contribs['ISOTP'] = {'use-can-isotp-kernel-module': False}
    load_contrib('isotp')
    load_contrib('automotive.uds')
    load_contrib('automotive.ecu')


    if args.protocol == 'uds':
        with ISOTPSocket(args.iface, tx_id=args.tx, rx_id=args.rx, basecls=UDS) as sock:
            resp = [] # TODO: Add responses
            ecu = EcuAnsweringMachine(supported_responses=resp, main_socket=sock, basecls=UDS)
            sim = threading.Thread(target=ecu, kwargs={'count': 4, 'timeout': args.timeout})
            sim.start()

    elif args.protocol == 'iso-tp':
        with ISOTPSocket(args.iface, tx_id=args.tx, rx_id=args.rx) as sock1:
            with ISOTPSocket(args.iface, tx_id=args.tx, rx_id=args.rx) as sock2:
                sock1.send(b'\xAA') # Signal to test that ECU is ready
                bridge_and_sniff(if1=sock1, if2=sock2, xfrm12=forwarding1, xfrm21=forwarding2, timeout=args.timeout)
