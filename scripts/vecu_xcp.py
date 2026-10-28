#!/usr/bin/env python3
"""Scapy XCP ECU for protocol integration tests, on CAN/CAN FD or TCP/UDP.

CTO requests are decoded by scapy.contrib.automotive.xcp.xcp; response
parameters are built with Scapy's XCP slave packet classes. Ethernet's four-byte
LEN/CTR header is encoded explicitly in little endian per ASAM XCP on Ethernet
1.1 section 1.3: Scapy 2.8's XCPOnTCP/XCPOnUDP fields use network byte order.
The transport loop never binds a physical CAN interface by default.
"""

from __future__ import annotations

import argparse
import json
import socket
import struct
import time
from collections import Counter

from scapy.config import conf
from scapy.contrib.automotive.xcp.cto_commands_master import (
    BuildChecksum,
    Connect,
    Download,
    GetId,
    GetSeed,
    SetMta,
    ShortUpload,
    Unlock,
    Upload,
)
from scapy.contrib.automotive.xcp.cto_commands_slave import (
    ChecksumPositiveResponse,
    CommonModeInfoPositiveResponse,
    ConnectPositiveResponse,
    GenericResponse,
    IdPositiveResponse,
    NegativeResponse,
    SeedPositiveResponse,
    ShortUploadPositiveResponse,
    StatusPositiveResponse,
    UnlockPositiveResponse,
    UploadPositiveResponse,
)
from scapy.contrib.automotive.xcp.xcp import CTORequest, CTOResponse, XCPOnCAN
from scapy.packet import Raw

BASE = 0x8000_0000
FLASH = bytes((i * 17 + 3) & 0xFF for i in range(4096))


class ECU:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.ag = args.ag
        self.base = args.base
        self.max_cto = (
            args.max_cto or (64 if args.fd else 8)
            if args.transport == "can"
            else args.max_cto or 32
        )
        self.memory = bytearray(FLASH)
        self.identity = b"SCAPYXCP"
        self.connected = False
        self.protection = 1 if args.locked else 0
        self.seed = bytes(range(19))
        self.seed_offset = 0
        self.key = bytearray()
        self.mta = self.base
        self.id_transfer = False
        self.stats: Counter[str] = Counter()
        self.requests: list[dict[str, int]] = []
        self.uploads: list[dict[str, int]] = []
        self.peers: set[tuple[str, int]] = set()
        self.tx_counter = 0x1234  # Independent of the master's counter.
        self.rx_counter = 0
        self.counter_gap_sent = False
        conf.contribs["XCP"].update(
            {
                "byte_order": int(args.byte_order == "big"),
                "Address_Granularity_Byte": self.ag,
                "MAX_CTO": self.max_cto,
                "MAX_DTO": self.max_cto,
                "add_padding_for_can": False,
            }
        )

    @staticmethod
    def positive(payload: object | None = None) -> bytes:
        return bytes(CTOResponse(packet_code=0xFF) / (payload or GenericResponse()))

    @staticmethod
    def negative(code: int) -> bytes:
        return bytes(CTOResponse(packet_code=0xFE) / NegativeResponse(error_code=code))

    def memory_read(self, address: int, size: int) -> bytes | None:
        offset = (address - self.base) * self.ag
        if offset < 0 or offset + size > len(self.memory):
            return None
        return bytes(self.memory[offset : offset + size])

    def handle(self, data: bytes) -> list[bytes]:
        assert 0 < len(data) <= self.max_cto
        request = CTORequest(data)
        pid = int(request.pid)
        payload = request.payload
        self.stats[f"{pid:02x}"] += 1
        if pid == 0xFF:
            assert isinstance(payload, Connect) and int(payload.connection_mode) in (
                0,
                1,
            )
            self.connected = True
            mode = (
                int(self.args.byte_order == "big") | {1: 0, 2: 2, 4: 4}[self.ag] | 0xC0
            )
            return [
                self.positive(
                    ConnectPositiveResponse(
                        resource=0x1D,
                        comm_mode_basic=mode,
                        max_cto=self.max_cto,
                        max_dto=self.max_cto,
                        max_dto_le=self.max_cto,
                    )
                )
            ]
        if not self.connected:
            return []
        if pid == 0xFC:
            return [self.negative(0)]
        if pid == 0xFE:
            self.connected = False
            return [self.positive()]
        if self.args.case == "negative":
            return [self.negative(0xA7)]
        if pid == 0xFD:
            if self.args.case == "silent":
                return []
            if self.args.case == "slow":
                time.sleep(0.15)
            result = self.positive(
                StatusPositiveResponse(
                    current_resource_protection_status=self.protection,
                    session_configuration_id=0x1234,
                )
            )
            if self.args.case == "malformed":
                return [result[:2]]
        elif pid == 0xFB:
            result = self.positive(
                CommonModeInfoPositiveResponse(
                    max_bs=4, min_st=1, queue_size=1, xcp_driver_version_number=0x15
                )
            )
        elif pid == 0xFA:
            assert isinstance(payload, GetId)
            self.id_transfer = True
            self.mta = 0
            inline = self.args.case == "id-inline"
            result = self.positive(
                IdPositiveResponse(
                    mode=int(inline),
                    length=len(self.identity),
                    element=self.identity if inline else b"",
                )
            )
        elif pid == 0xF8:
            assert isinstance(payload, GetSeed)
            if int(payload.mode) == 0:
                self.seed_offset = 0
            remaining = len(self.seed) - self.seed_offset if self.protection else 0
            part = self.seed[
                self.seed_offset : self.seed_offset + min(remaining, self.max_cto - 2)
            ]
            self.seed_offset += len(part)
            result = self.positive(
                SeedPositiveResponse(seed_length=remaining, seed=part)
            )
        elif pid == 0xF7:
            assert isinstance(payload, Unlock)
            assert int(payload.len) == 10 - len(self.key)
            self.key.extend(bytes(payload.seed))
            if len(self.key) == 10:
                assert self.key == bytes(range(10))
                self.protection = 0
            result = self.positive(
                UnlockPositiveResponse(
                    current_resource_protection_status=self.protection
                )
            )
        elif pid == 0xF6:
            assert isinstance(payload, SetMta)
            assert int(payload.address_extension) == self.args.address_extension
            self.mta = int(payload.address)
            self.id_transfer = False
            result = self.positive()
        elif pid in (0xF4, 0xF5):
            assert isinstance(payload, ShortUpload if pid == 0xF4 else Upload)
            count = int(payload.nr_of_data_elements)
            assert 0 < count <= self.max_cto // self.ag - 1
            if pid == 0xF4:
                assert int(payload.address_extension) == self.args.address_extension
                self.mta = int(payload.address)
                self.id_transfer = False
                self.requests.append({"address": self.mta, "elements": count})
            else:
                self.uploads.append({"address": self.mta, "elements": count})
            size = count * self.ag
            if self.id_transfer:
                content = self.identity[self.mta * self.ag : self.mta * self.ag + size]
            else:
                content = self.memory_read(self.mta, size)
            if content is None:
                return [self.negative(0x24)]
            assert len(content) == size
            self.mta += count
            if (
                self.args.case == "upload"
                and pid == 0xF5
                and self.mta == self.base + len(self.memory) // self.ag
            ):
                self.connected = False
            cls = ShortUploadPositiveResponse if pid == 0xF4 else UploadPositiveResponse
            result = self.positive(
                cls(alignment=b"\x00" * (self.ag - 1), element=content)
            )
        elif pid == 0xF0:
            assert isinstance(payload, Download)
            count = data[1]
            offset = max(2, self.ag)
            content = data[offset : offset + count * self.ag]
            assert len(content) == count * self.ag
            if self.protection:
                return [self.negative(0x25)]
            index = (self.mta - self.base) * self.ag
            if index < 0 or index + len(content) > len(self.memory):
                return [self.negative(0x24)]
            self.memory[index : index + len(content)] = content
            self.mta += count
            result = self.positive()
        elif pid == 0xF3:
            assert isinstance(payload, BuildChecksum)
            content = self.memory_read(self.mta, int(payload.block_size) * self.ag)
            if content is None:
                return [self.negative(0x24)]
            result = self.positive(
                ChecksumPositiveResponse(checksum_type=3, checksum=sum(content))
            )
        else:
            return [self.negative(0x20)]
        if self.args.case == "events" and pid == 0xF4:
            return [bytes(CTOResponse(packet_code=0xFD) / Raw(b"\x06")), result]
        return [result]

    def ethernet(self, response: bytes) -> bytes:
        counter = self.tx_counter
        if (
            self.args.case == "counter-gap"
            and self.stats["fd"]
            and not self.counter_gap_sent
        ):
            counter = (counter + 1) & 0xFFFF
            self.counter_gap_sent = True
        self.tx_counter = (counter + 1) & 0xFFFF
        return struct.pack("<HH", len(response), counter) + response

    def check_header(self, header: bytes) -> int:
        length, counter = struct.unpack("<HH", header)
        assert 0 < length <= self.max_cto
        assert counter == self.rx_counter, (counter, self.rx_counter)
        self.rx_counter = (counter + 1) & 0xFFFF
        return length


def exact(connection: socket.socket, length: int) -> bytes:
    result = bytearray()
    while len(result) < length:
        part = connection.recv(length - len(result))
        if not part:
            raise EOFError
        result.extend(part)
    return bytes(result)


def serve_tcp(ecu: ECU) -> None:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as server:
        server.bind(("127.0.0.1", ecu.args.port))
        server.listen(1)
        print(json.dumps({"port": server.getsockname()[1]}), flush=True)
        connection, _ = server.accept()
        with connection:
            connection.settimeout(30)
            connection.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            while True:
                try:
                    size = ecu.check_header(exact(connection, 4))
                    responses = ecu.handle(exact(connection, size))
                    reply = b"".join(ecu.ethernet(r) for r in responses)
                    if ecu.args.case == "fragmented" and reply:
                        # Split in the middle of LEN, CTR and the CTO payload.
                        for part in (reply[:1], reply[1:3], reply[3:5], reply[5:]):
                            connection.sendall(part)
                            time.sleep(0.001)
                    elif reply:
                        connection.sendall(reply)
                    if not ecu.connected:
                        return
                except (EOFError, ConnectionResetError, BrokenPipeError):
                    return


def serve_udp(ecu: ECU) -> None:
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as server:
        server.bind(("127.0.0.1", ecu.args.port))
        server.settimeout(30)
        print(json.dumps({"port": server.getsockname()[1]}), flush=True)
        while True:
            datagram, peer = server.recvfrom(65536)
            ecu.peers.add(peer)
            offset = 0
            responses = []
            while offset < len(datagram):
                size = ecu.check_header(datagram[offset : offset + 4])
                assert offset + 4 + size <= len(datagram)
                responses.extend(ecu.handle(datagram[offset + 4 : offset + 4 + size]))
                offset += 4 + size
            if responses:
                server.sendto(b"".join(ecu.ethernet(r) for r in responses), peer)
            if not ecu.connected:
                return


def serve_can(ecu: ECU) -> None:
    with socket.socket(socket.PF_CAN, socket.SOCK_RAW, socket.CAN_RAW) as server:
        server.setsockopt(socket.SOL_CAN_RAW, socket.CAN_RAW_FD_FRAMES, 1)
        extended = 0x8000_0000 if ecu.args.extended else 0
        filters = struct.pack(
            "=II",
            ecu.args.rx | extended,
            socket.CAN_EFF_FLAG
            | socket.CAN_RTR_FLAG
            | (socket.CAN_EFF_MASK if ecu.args.extended else socket.CAN_SFF_MASK),
        )
        if ecu.args.require_wakeup:
            filters += struct.pack(
                "=II",
                0x541,
                socket.CAN_EFF_FLAG | socket.CAN_RTR_FLAG | socket.CAN_SFF_MASK,
            )
        server.setsockopt(socket.SOL_CAN_RAW, socket.CAN_RAW_FILTER, filters)
        server.bind((ecu.args.iface,))
        server.settimeout(30)
        print(json.dumps({"interface": ecu.args.iface}), flush=True)
        while True:
            wire = server.recv(72)
            identifier, length, flags = struct.unpack("=IBB2x", wire[:8])
            if identifier == 0x541:
                assert len(wire) == 16 and length == 8 and wire[8:16] == b"\xff" * 8
                ecu.stats["wake541"] += 1
                continue
            if ecu.args.require_wakeup and ecu.stats["wake541"] < 10:
                continue
            assert bool(len(wire) == 72) == ecu.args.fd
            if ecu.args.fd:
                assert flags & 1  # BRS must survive the client's frame builder.
            # Use Scapy's CAN binding as well as its CTO request decoder.
            frame = XCPOnCAN(
                identifier=identifier & 0x1FFF_FFFF, length=length
            ) / CTORequest(wire[8 : 8 + length])
            assert int(frame.identifier) == ecu.args.rx
            for response in ecu.handle(bytes(frame.payload)):
                if ecu.args.fd:
                    padded = next(
                        n
                        for n in (0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64)
                        if n >= len(response)
                    )
                    packet = struct.pack(
                        "=IBB2x64s",
                        ecu.args.tx | extended,
                        padded,
                        1,
                        response.ljust(padded, b"\xcc"),
                    )
                else:
                    packet = struct.pack(
                        "=IB3x8s", ecu.args.tx | extended, 8, response.ljust(8, b"\xcc")
                    )
                server.send(packet)
            if not ecu.connected:
                return


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--transport", choices=("can", "tcp", "udp"), default="tcp")
    parser.add_argument("--iface", default="vcan0")
    parser.add_argument("--rx", type=lambda x: int(x, 0), default=0x6C3)
    parser.add_argument("--tx", type=lambda x: int(x, 0), default=0x6C4)
    parser.add_argument("--fd", action="store_true")
    parser.add_argument("--extended", action="store_true")
    parser.add_argument("--locked", action="store_true")
    parser.add_argument("--require-wakeup", action="store_true")
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--max-cto", type=int)
    parser.add_argument("--ag", type=int, choices=(1, 2, 4), default=1)
    parser.add_argument("--byte-order", choices=("little", "big"), default="little")
    parser.add_argument("--address-extension", type=lambda x: int(x, 0), default=0)
    parser.add_argument("--base", type=lambda x: int(x, 0), default=BASE)
    parser.add_argument(
        "--case",
        choices=(
            "normal",
            "fragmented",
            "events",
            "counter-gap",
            "negative",
            "silent",
            "malformed",
            "slow",
            "id-inline",
            "upload",
        ),
        default="normal",
    )
    parser.add_argument("--stats")
    args = parser.parse_args()
    ecu = ECU(args)
    try:
        {"can": serve_can, "tcp": serve_tcp, "udp": serve_udp}[args.transport](ecu)
    finally:
        if args.stats:
            with open(args.stats, "w") as output:
                json.dump(
                    {
                        "commands": ecu.stats,
                        "short_uploads": ecu.requests,
                        "uploads": ecu.uploads,
                        "peers": sorted(ecu.peers),
                    },
                    output,
                )


if __name__ == "__main__":
    main()
