#!/usr/bin/env python3
"""Scapy CCP ECU for protocol tests, bound only to a virtual CAN interface.

Scapy decodes CROs and builds DTOs. Its address fields are big endian, so
little-endian ECU addresses are converted at the packet boundary.
"""

from __future__ import annotations

import argparse
import json
import socket
import struct
from pathlib import Path

from scapy.contrib.automotive import ccp

BASE = 0x8000_0000
MEMORY = bytes((i * 17 + 3) & 0xFF for i in range(4096))


class ECU:
    def __init__(self, byte_order: str) -> None:
        self.byte_order = byte_order
        self.memory = bytearray(MEMORY)
        self.connected = False
        self.locked = True
        self.mta = BASE
        self.extension = 0
        self.identity = False
        self.commands: list[int] = []

    def address(self, value: int) -> int:
        return int.from_bytes(value.to_bytes(4, "big"), self.byte_order)

    def scapy_address(self, value: int) -> int:
        return int.from_bytes(value.to_bytes(4, self.byte_order), "big")

    def handle(self, data: bytes) -> bytes:
        assert len(data) == 8
        request = ccp.CRO(data)
        cmd = int(request.cmd)
        self.commands.append(cmd)
        payload = request.payload
        response = ccp.DTO(ctr=request.ctr)

        def reject(code: int) -> bytes:
            return bytes(ccp.DTO(ctr=request.ctr, return_code=code) / ccp.DEFAULT_DTO())

        def read(address: int, size: int) -> bytes:
            offset = address - BASE
            assert 1 <= size <= 5 and 0 <= offset <= len(self.memory) - size
            return bytes(self.memory[offset : offset + size])

        if cmd == 0x01:
            assert (
                isinstance(payload, ccp.CONNECT) and payload.station_address == 0x1234
            )
            self.connected = True
        elif not self.connected:
            return reject(0x33)
        elif cmd == 0x1B:
            assert isinstance(payload, ccp.GET_CCP_VERSION)
            assert (payload.main_protocol_version, payload.release_version) == (2, 1)
            return bytes(
                response
                / ccp.GET_CCP_VERSION_DTO(main_protocol_version=2, release_version=1)
            )
        elif cmd == 0x17:
            assert isinstance(payload, ccp.EXCHANGE_ID)
            self.identity = True
            return bytes(
                response
                / ccp.EXCHANGE_ID_DTO(
                    slave_device_ID_length=5,
                    data_type_qualifier=0,
                    resource_availability_mask=0x43,
                    resource_protection_mask=int(self.locked),
                )
            )
        elif cmd == 0x12:
            assert isinstance(payload, ccp.GET_SEED) and payload.resource == 1
            return bytes(
                response
                / ccp.GET_SEED_DTO(protection_status=int(self.locked), seed=b"seed")
            )
        elif cmd == 0x13:
            assert isinstance(payload, ccp.UNLOCK) and payload.key == b"key!\x00\x00"
            self.locked = False
            return bytes(response / ccp.UNLOCK_DTO(privilege_status=1))
        elif cmd == 0x02:
            assert isinstance(payload, ccp.SET_MTA) and payload.mta_num == 0
            self.mta = self.address(int(payload.address))
            self.extension = int(payload.address_extension)
            self.identity = False
        elif cmd in (0x03, 0x23):
            assert isinstance(payload, (ccp.DNLOAD, ccp.DNLOAD_6))
            if self.locked:
                return reject(0x35)
            size = int(payload.size) if cmd == 0x03 else 6
            assert 1 <= size <= (5 if cmd == 0x03 else 6)
            offset = self.mta - BASE
            assert 0 <= offset <= len(self.memory) - size
            self.memory[offset : offset + size] = bytes(payload.data)[:size]
            self.mta += size
            dto = ccp.DNLOAD_DTO if cmd == 0x03 else ccp.DNLOAD_6_DTO
            return bytes(
                response
                / dto(
                    MTA0_extension=self.extension,
                    MTA0_address=self.scapy_address(self.mta),
                )
            )
        elif cmd == 0x04:
            assert isinstance(payload, ccp.UPLOAD)
            if self.identity:
                assert payload.size == 5
                result = b"SCAPY"
            else:
                result = read(self.mta, int(payload.size))
                self.mta += int(payload.size)
            return bytes(response / ccp.UPLOAD_DTO(data=result))
        elif cmd == 0x0F:
            assert isinstance(payload, ccp.SHORT_UP)
            assert payload.address_extension == 0x5A
            result = read(self.address(int(payload.address)), int(payload.size))
            return bytes(response / ccp.SHORT_UP_DTO(data=result))
        elif cmd == 0x07:
            assert (
                isinstance(payload, ccp.DISCONNECT)
                and payload.station_address == 0x1234
            )
            self.connected = False
        else:
            return reject(0x30)
        return bytes(response / ccp.DEFAULT_DTO())


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--interface", default="vcan0")
    parser.add_argument("--byte-order", choices=("little", "big"), default="little")
    parser.add_argument("--extended", action="store_true")
    parser.add_argument("--stats", required=True, type=Path)
    args = parser.parse_args()
    if not args.interface.startswith("vcan"):
        parser.error("CCP test ECU requires a virtual CAN interface")
    ecu = ECU(args.byte_order)
    sock = socket.socket(socket.AF_CAN, socket.SOCK_RAW, socket.CAN_RAW)
    sock.bind((args.interface,))
    sock.settimeout(10)
    flag = socket.CAN_EFF_FLAG if args.extended else 0
    print(json.dumps({"ready": True}), flush=True)
    try:
        while True:
            frame = sock.recv(16)
            can_id, size, data = struct.unpack("=IB3x8s", frame)
            if can_id != (0x6EC | flag):
                continue
            result = ecu.handle(data[:size])
            assert len(result) == 8
            sock.send(struct.pack("=IB3x8s", 0x6ED | flag, 8, result))
            if ecu.commands[-1] == 0x07:
                break
    finally:
        sock.close()
        args.stats.write_text(json.dumps({"commands": ecu.commands}))


if __name__ == "__main__":
    main()
