#!/usr/bin/env python3
"""Serve the shared UDS answering machine over CAN ISO-TP."""

import argparse

from scapy.all import conf
from uds_ecu import SUPPRESSION_CASES, SuppressionAwareECU, ecu_responses


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--iface", type=str, default="vcan0")
    parser.add_argument("--rx", type=int, default=0x7a1)
    parser.add_argument("--tx", type=int, default=0x7a9)
    parser.add_argument("--timeout", type=int, default=10)
    parser.add_argument("--kernel-isotp", type=bool, default=True)
    parser.add_argument("--suppression-case", choices=SUPPRESSION_CASES)

    args = parser.parse_args()

    conf.contribs['ISOTP'] = {'use-can-isotp-kernel-module': args.kernel_isotp}
    from scapy.contrib.isotp import ISOTPSocket
    from scapy.contrib.automotive.uds import UDS


    with ISOTPSocket(args.iface, tx_id=args.tx, rx_id=args.rx, basecls=UDS) as isotp:
        ecu = SuppressionAwareECU(supported_responses=ecu_responses(),
            main_socket=isotp, basecls=UDS, verbose=False, timeout=args.timeout,
            suppression_case=args.suppression_case)
        isotp.send(b'\xAA')  # The response definitions and socket are ready.
        ecu()
