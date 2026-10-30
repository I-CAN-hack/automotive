#!/usr/bin/env python3
"""Loopback DoIP ECU: Scapy framing and the shared EcuAnsweringMachine responses.

Uses ordinary TCP/UDP sockets, so it runs on Windows without raw-socket access.
Prints READY <port> after both sockets and the Scapy response definitions are ready.
"""

import argparse
import selectors
import socket
import threading
import time

from scapy.contrib.automotive.doip import DoIP
from scapy.contrib.automotive.uds import UDS
from uds_ecu import SUPPRESSION_CASES, VIN, SuppressionAwareECU, ecu_responses

EID = b"\x00\x11\x22\x33\x44\x55"
GID = b"\xaa\xbb\xcc\xdd\xee\xff"
ECU_ADDRESS = 0x1000


def packet(version, kind, **fields):
    return DoIP(protocol_version=version, inverse_version=version ^ 0xFF, payload_type=kind, **fields)


def read_packet(stream):
    def exact(count):
        data = bytearray()
        while len(data) < count:
            chunk = stream.recv(count - len(data))
            if not chunk:
                raise EOFError
            data.extend(chunk)
        return bytes(data)

    header = exact(8)
    return DoIP(header + exact(int.from_bytes(header[4:8], "big")))


def serve_connection(stream, args):
    with stream:
        # Send ACKs and spaced UDS replies promptly: Nagle plus delayed TCP
        # acknowledgements can otherwise push replies past P2/P2* deadlines.
        stream.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        stream.settimeout(10)
        activation = read_packet(stream)
        assert activation.payload_type == 5
        assert activation.reserved_iso == 0
        source = activation.source_address
        version = activation.protocol_version
        code = {"activation-denied": 6, "confirmation": 0x11, "tls-required": 7}.get(args.case, 0x10)
        stream.sendall(bytes(packet(version, 6, logical_address_tester=source,
                                    logical_address_doip_entity=0x2000, routing_activation_response=code)))
        if code != 0x10:
            return

        # The client must answer this even before any recv() or UDS call.
        time.sleep(0.02)
        stream.sendall(bytes(packet(version, 7)))
        # A diagnostic request can cross the alive request on the wire.
        # Both must be accepted while the client answers the alive check.
        machine = SuppressionAwareECU(supported_responses=ecu_responses(), basecls=UDS,
                                      verbose=False, suppression_case=args.suppression_case)
        alive_seen = False
        while True:
            request = read_packet(stream)
            if request.payload_type == 8:
                assert request.source_address == source
                alive_seen = True
                continue
            assert request.payload_type == 0x8001
            if args.case == "require-idle-alive":
                assert alive_seen, "client did not answer the idle alive check"
            assert request.source_address == source
            target = request.target_address
            data = bytes(request.payload)
            ack_fields = dict(source_address=target, target_address=source, previous_msg=data)
            if args.case == "no-ack":
                continue
            if args.case == "header-nack":
                stream.sendall(bytes(packet(version, 0, nack=4)))
                continue
            if args.case == "nack":
                stream.sendall(bytes(packet(version, 0x8003, nack_code=3, **ack_fields)))
                continue
            if args.case == "bad-ack":
                ack_fields["previous_msg"] = b"\xff"
            ack = bytes(packet(version, 0x8002, ack_code=0, **ack_fields))
            if args.case == "oversize":
                # Reject from the header, without waiting for or allocating its body.
                stream.sendall(bytes(packet(version, 0x8001, source_address=target,
                                            target_address=source, payload_length=0xFFFFFFFF)))
                continue
            if args.case == "bad-version":
                stream.sendall(bytes([version, version]) + ack[2:])
                continue
            if args.case == "wrong-address":
                ack = bytes(packet(version, 0x8002, ack_code=0, source_address=target ^ 1,
                                   target_address=source, previous_msg=data)) + ack
                ack += bytes(packet(version, 0x8001, source_address=target ^ 1,
                                    target_address=source) / UDS(b"\x62\x12\x34wrong"))

            replies = machine.make_reply(UDS(data))
            def encode_response(reply):
                return bytes(packet(version, 0x8001, source_address=target,
                                    target_address=source) / reply)

            if len(replies) > 1:
                # Share UDS response-pending spacing with the CAN ECU. A normal
                # single response still shares a TCP write with its DoIP ACK.
                if args.case != "response-before-ack":
                    stream.sendall(ack)
                machine.send_reply(replies, send_function=lambda reply: stream.sendall(encode_response(reply)))
                if args.case == "response-before-ack":
                    stream.sendall(ack)
                continue
            response = b"".join(encode_response(reply) for reply in replies)
            output = response + ack if args.case == "response-before-ack" else ack + response
            if args.case == "fragmented":
                for offset in range(0, len(output), 3):
                    stream.sendall(output[offset:offset + 3])
                    time.sleep(0.002)
            else:
                stream.sendall(output)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--case", default="normal")
    parser.add_argument("--suppression-case", choices=SUPPRESSION_CASES)
    parser.add_argument("--announce-to", help="Unicast test destination port for passive reception")
    args = parser.parse_args()
    family = socket.AF_INET6 if ":" in args.host else socket.AF_INET
    with socket.socket(family, socket.SOCK_STREAM) as tcp, socket.socket(family, socket.SOCK_DGRAM) as udp:
        if family == socket.AF_INET6:
            tcp.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
            udp.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
        tcp.bind((args.host, args.port))
        port = tcp.getsockname()[1]
        udp.bind((args.host, port))
        tcp.listen()
        selector = selectors.DefaultSelector()
        selector.register(tcp, selectors.EVENT_READ)
        selector.register(udp, selectors.EVENT_READ)
        # Build the definitions before publishing readiness.
        ecu_responses()
        print(f"READY {port}", flush=True)
        if args.announce_to:
            time.sleep(0.02)
            udp.sendto(bytes(packet(2, 4, vin=VIN, logical_address=ECU_ADDRESS, eid=EID,
                                   gid=GID, further_action=0, vin_gid_status=0)), (args.host, int(args.announce_to)))
        while True:
            for key, _ in selector.select():
                if key.fileobj is tcp:
                    stream, _ = tcp.accept()
                    def session(connection):
                        try:
                            serve_connection(connection, args)
                        except (EOFError, ConnectionError, TimeoutError):
                            pass
                    threading.Thread(target=session, args=(stream,), daemon=True).start()
                else:
                    data, source = udp.recvfrom(65535)
                    request = DoIP(data)
                    if request.payload_type == 2 and request.eid != EID:
                        continue
                    if request.payload_type == 3 and request.vin != VIN:
                        continue
                    if request.payload_type not in {1, 2, 3}:
                        continue
                    assert request.protocol_version in {2, 3, 4, 255}
                    assert request.inverse_version == request.protocol_version ^ 255
                    announcement = bytes(packet(2, 4, vin=VIN, logical_address=ECU_ADDRESS,
                                                eid=EID, gid=GID, further_action=0, vin_gid_status=0))
                    # Exercise datagram validation and repeated announcements.
                    udp.sendto(announcement[:-1], source)  # Declared length is now wrong.
                    udp.sendto(announcement + announcement, source)  # Two PDUs in one datagram.
                    udp.sendto(announcement, source)
                    udp.sendto(announcement, source)


if __name__ == "__main__":
    main()
