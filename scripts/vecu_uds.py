#!/usr/bin/env python3
import argparse
import threading

from scapy.all import *

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--iface", type=str, default="vcan0")
    parser.add_argument("--rx", type=int, default=0x7a1)
    parser.add_argument("--tx", type=int, default=0x7a9)
    parser.add_argument("--timeout", type=int, default=10)
    parser.add_argument("--kernel-isotp", type=bool, default=True)

    args = parser.parse_args()

    conf.contribs['ISOTP'] = {'use-can-isotp-kernel-module': args.kernel_isotp}
    load_contrib('isotp')
    load_contrib('automotive.uds')
    load_contrib('automotive.ecu')

    with ISOTPSocket(args.iface, tx_id=args.tx, rx_id=args.rx, basecls=UDS) as isotp:
        isotp.send(b'\xAA') # Signal to test that ECU is ready

        resp = [
            EcuResponse([EcuState(session=range(0,255))], responses=UDS() / UDS_TPPR()),
            EcuResponse([EcuState(session=range(0,255))], responses=UDS() / UDS_RDBIPR(dataIdentifier=0x1234) / Raw(b"deadbeef")),
            EcuResponse([EcuState(session=range(0,255))], responses=UDS() /  UDS_NR(negativeResponseCode=0x33, requestServiceId=0x10)),
        ]
        ecu = EcuAnsweringMachine(supported_responses=resp, main_socket=isotp, basecls=UDS, verbose=False, timeout=args.timeout)
        ecu()
