"""Wire interoperability with Scapy's XCP CTO decoder/response encoders."""

from __future__ import annotations

import asyncio
import json
import sys
import tempfile
from contextlib import asynccontextmanager
from pathlib import Path

import pytest

from automotive.adapters import SocketCanAdapter
from automotive.can import Identifier
from automotive.errors import XCPError, XCPResponseError
from automotive.protocols.xcp import Resource, XCPCanConfig, XCPClient, XCPConfig, XCPOnCAN, XCPOnTCP, XCPOnUDP
from automotive.sync.protocols.xcp import XCPClient as SyncXCPClient
from automotive.sync.protocols.xcp import XCPOnTCP as SyncTCP
from automotive.sync.protocols.xcp import XCPOnUDP as SyncUDP

ROOT = Path(__file__).resolve().parents[2]
BASE = 0x8000_0000
FLASH = bytes((i * 17 + 3) & 0xFF for i in range(4096))


@asynccontextmanager
async def ecu(tmp_path: Path, transport: str = "tcp", *options: str):
    stats = tmp_path / "stats.json"
    with tempfile.TemporaryFile() as stderr:
        process = await asyncio.create_subprocess_exec(
            sys.executable,
            str(ROOT / "scripts/vecu_xcp.py"),
            "--transport",
            transport,
            "--stats",
            str(stats),
            *options,
            stdout=asyncio.subprocess.PIPE,
            stderr=stderr,
        )
        try:
            assert process.stdout is not None
            ready = await asyncio.wait_for(process.stdout.readline(), 10)
            assert ready, "Scapy ECU exited before announcing readiness"
            yield json.loads(ready), process, stats
        finally:
            if process.returncode is None:
                process.terminate()
            await asyncio.wait_for(process.wait(), 5)
            stderr.seek(0)
            output = stderr.read().decode(errors="replace")
            assert process.returncode in (0, -15), output


@pytest.mark.parametrize("transport", ["tcp", "udp"])
@pytest.mark.parametrize("byte_order,ag", [("little", 1), ("big", 1), ("little", 2), ("big", 4)])
async def test_scapy_flash_read_byte_order_and_granularity(
    tmp_path: Path, transport: str, byte_order: str, ag: int
) -> None:
    async with ecu(tmp_path, transport, "--byte-order", byte_order, "--ag", str(ag), "--address-extension", "0x5a") as (
        ready,
        process,
        stats,
    ):
        factory = tcp_client if transport == "tcp" else udp_client
        async with await factory("127.0.0.1", ready["port"]) as client:
            info = await client.connect()
            assert info.byte_order == byte_order and info.address_granularity == ag
            assert info.max_cto == 32 and info.max_dto == 32
            assert info.max_upload_size == 32 - ag
            data = bytearray()
            while len(data) < 4096:
                size = min(info.max_upload_size, 4096 - len(data))
                data.extend(await client.short_upload(BASE + len(data) // ag, size, 0x5A))
            assert data == FLASH
            await client.disconnect()
        await asyncio.wait_for(process.wait(), 2)
        report = json.loads(stats.read_text())
        assert set(report["commands"]) == {"ff", "f4", "fe"}
        offset = 0
        for request in report["short_uploads"]:
            assert request["address"] == BASE + offset // ag
            offset += request["elements"] * ag
        assert offset == 4096


@pytest.mark.vcan
@pytest.mark.parametrize(
    "fd,extended,byte_order,ag",
    [
        (False, False, "little", 1),
        (False, True, "big", 2),
        (True, False, "little", 1),
        (True, True, "big", 4),
    ],
)
async def test_scapy_vcan_classic_and_fd(tmp_path: Path, fd: bool, extended: bool, byte_order: str, ag: int) -> None:
    options = ["--byte-order", byte_order, "--ag", str(ag)]
    if fd:
        options += ["--fd"]
    if extended:
        options += ["--extended"]
    async with ecu(tmp_path, "can", *options) as (_, process, stats):
        make_id = Identifier.extended if extended else Identifier.standard
        can = XCPCanConfig(tx_id=make_id(0x6C3), rx_id=make_id(0x6C4), fd=fd)
        async with SocketCanAdapter("vcan0") as adapter, XCPClient(XCPOnCAN(adapter, can)) as client:
            info = await client.connect()
            assert info.max_cto == (64 if fd else 8)
            data = bytearray()
            while len(data) < 4096:
                size = min(info.max_upload_size, 4096 - len(data))
                data.extend(await client.short_upload(BASE + len(data) // ag, size))
            assert data == FLASH
            await client.disconnect()
        await asyncio.wait_for(process.wait(), 2)
        report = json.loads(stats.read_text())
        assert set(report["commands"]) == {"ff", "f4", "fe"}


async def test_scapy_individual_commands_and_explicit_seed_key_segments(tmp_path: Path) -> None:
    async with (
        ecu(tmp_path, "tcp", "--max-cto", "8", "--locked") as (ready, process, stats),
        await tcp_client("127.0.0.1", ready["port"]) as client,
    ):
        await client.connect()
        assert (await client.get_status()).session_configuration_id == 0x1234
        assert (await client.get_status()).resource_protection_status == 1
        assert (await client.get_comm_mode_info()).driver_version == 0x15
        for mode, offset in [(0, 0), (1, 6), (1, 12), (1, 18)]:
            seed = await client.get_seed(mode, Resource.CAL_PAG)
            assert seed.length == 19 - offset
            assert seed.data == bytes(range(offset, min(offset + 6, 19)))
        assert await client.unlock(10, memoryview(bytes(range(6)))) == 1
        assert await client.unlock(4, bytes(range(6, 10))) == 0
        identity = await client.get_id()
        assert identity.mode == 0 and identity.length == 8
        assert await client.upload(7) == b"SCAPYXC"
        assert await client.upload(1) == b"P"
        data = bytes(range(40))
        await client.set_mta(BASE)
        for offset in range(0, len(data), 6):
            await client.download(data[offset : offset + 6])
        await client.set_mta(BASE)
        uploaded = bytearray()
        while len(uploaded) < len(data):
            uploaded.extend(await client.upload(min(7, len(data) - len(uploaded))))
        assert uploaded == data
        await client.set_mta(BASE)
        checksum = await client.build_checksum(len(data))
        assert checksum.checksum_type == 3 and checksum.checksum == sum(data)
        await client.disconnect()
        await asyncio.wait_for(process.wait(), 2)
        report = json.loads(stats.read_text())["commands"]
        assert report["f8"] == 4 and report["f7"] == 2
        assert report["f0"] == 7 and report["f5"] == 8


@pytest.mark.parametrize("ag", [2, 4])
async def test_scapy_download_alignment(tmp_path: Path, ag: int) -> None:
    async with (
        ecu(tmp_path, "tcp", "--max-cto", "12", "--ag", str(ag), "--byte-order", "big") as (ready, _, _),
        await tcp_client("127.0.0.1", ready["port"]) as client,
    ):
        await client.connect()
        data = bytes(range(40))
        await client.set_mta(BASE)
        capacity = (12 - max(2, ag)) // ag * ag
        for offset in range(0, len(data), capacity):
            await client.download(data[offset : offset + capacity])
        await client.set_mta(BASE)
        uploaded = bytearray()
        while len(uploaded) < len(data):
            uploaded.extend(await client.upload(min(12 - ag, len(data) - len(uploaded))))
        assert uploaded == data
        await client.disconnect()


@pytest.mark.parametrize("transport", ["tcp", "udp"])
async def test_scapy_events_cannot_satisfy_commands(tmp_path: Path, transport: str) -> None:
    async with ecu(tmp_path, transport, "--case", "events") as (ready, _, _):
        factory = tcp_client if transport == "tcp" else udp_client
        async with await factory("127.0.0.1", ready["port"]) as client:
            await client.connect()
            assert await client.short_upload(BASE, 7) == FLASH[:7]
            await client.disconnect()


async def test_scapy_fragmented_tcp_headers_and_payloads(tmp_path: Path) -> None:
    async with (
        ecu(tmp_path, "tcp", "--case", "fragmented") as (ready, _, _),
        await tcp_client("127.0.0.1", ready["port"]) as client,
    ):
        info = await client.connect()
        data = bytearray()
        while len(data) < 128:
            size = min(info.max_upload_size, 128 - len(data))
            data.extend(await client.short_upload(BASE + len(data), size))
        assert data == FLASH[:128]
        await client.disconnect()


async def test_scapy_inline_identification(tmp_path: Path) -> None:
    async with (
        ecu(tmp_path, "tcp", "--case", "id-inline") as (ready, _, _),
        await tcp_client("127.0.0.1", ready["port"]) as client,
    ):
        await client.connect()
        identity = await client.get_id()
        assert identity.mode == 1 and identity.length == 8 and identity.data == b"SCAPYXCP"
        await client.disconnect()


@pytest.mark.parametrize("transport", ["tcp", "udp"])
async def test_scapy_ethernet_counter_gap_is_reported(tmp_path: Path, transport: str) -> None:
    async with ecu(tmp_path, transport, "--case", "counter-gap") as (ready, _, _):
        factory = tcp_client if transport == "tcp" else udp_client
        async with await factory("127.0.0.1", ready["port"]) as client:
            await client.connect()
            with pytest.raises(XCPError, match="counter mismatch"):
                await client.get_status()
            with pytest.raises(XCPError, match="synchronize"):
                await client.short_upload(BASE, 1)
            await client.synchronize()
            assert await client.short_upload(BASE, 1) == FLASH[:1]
            await client.disconnect()


async def test_scapy_timeout_requires_synch_and_never_retries(tmp_path: Path) -> None:
    async with (
        ecu(tmp_path, "tcp", "--case", "silent") as (ready, process, stats),
        await tcp_client("127.0.0.1", ready["port"], XCPConfig(timeout=0.1)) as client,
    ):
        await client.connect()
        with pytest.raises(TimeoutError):
            await client.get_status()
        with pytest.raises(XCPError, match="synchronize"):
            await client.short_upload(BASE, 1)
        await client.synchronize()
        assert await client.short_upload(BASE, 1) == FLASH[:1]
        await client.disconnect()
        await asyncio.wait_for(process.wait(), 2)
        assert json.loads(stats.read_text())["commands"]["fd"] == 1


async def test_scapy_cancelled_exchange_requires_explicit_synch(tmp_path: Path) -> None:
    async with (
        ecu(tmp_path, "tcp", "--case", "slow") as (ready, _, _),
        await tcp_client("127.0.0.1", ready["port"]) as client,
    ):
        await client.connect()
        pending = asyncio.create_task(client.get_status())
        await asyncio.sleep(0.05)
        pending.cancel()
        with pytest.raises(asyncio.CancelledError):
            await pending
        with pytest.raises(XCPError, match="synchronize"):
            await client.short_upload(BASE, 7)
        await client.synchronize()
        assert await client.short_upload(BASE, 7) == FLASH[:7]
        await client.disconnect()


async def test_scapy_malformed_response_retains_packet(tmp_path: Path) -> None:
    async with (
        ecu(tmp_path, "tcp", "--case", "malformed") as (ready, _, _),
        await tcp_client("127.0.0.1", ready["port"]) as client,
    ):
        await client.connect()
        with pytest.raises(XCPError) as error:
            await client.get_status()
        assert error.value.response == b"\xff\x00"
        await client.synchronize()
        await client.disconnect()


async def test_scapy_unknown_negative_response_is_preserved(tmp_path: Path) -> None:
    async with (
        ecu(tmp_path, "tcp", "--case", "negative") as (ready, _, _),
        await tcp_client("127.0.0.1", ready["port"]) as client,
    ):
        await client.connect()
        with pytest.raises(XCPResponseError) as error:
            await client.get_status()
        assert error.value.command == 0xFD and error.value.code == 0xA7
        await client.disconnect()


@pytest.mark.parametrize("transport", ["tcp", "udp"])
async def test_scapy_synchronous_client(tmp_path: Path, transport: str) -> None:
    async with ecu(tmp_path, transport) as (ready, _, _):

        def exchange() -> None:
            factory = sync_tcp_client if transport == "tcp" else sync_udp_client
            with factory("127.0.0.1", ready["port"]) as client:
                info = client.connect()
                client.set_mta(BASE)
                data = bytearray()
                while len(data) < 4096:
                    data.extend(client.upload(min(info.max_upload_size, 4096 - len(data))))
                assert data == FLASH
                client.disconnect()

        await asyncio.to_thread(exchange)


async def tcp_client(host: str, port: int, config: XCPConfig | None = None) -> XCPClient:
    return XCPClient(await XCPOnTCP.connect(host, port), config)


async def udp_client(host: str, port: int, config: XCPConfig | None = None) -> XCPClient:
    return XCPClient(await XCPOnUDP.connect(host, port), config)


def sync_tcp_client(host: str, port: int) -> SyncXCPClient:
    return SyncXCPClient(SyncTCP.connect(host, port))


def sync_udp_client(host: str, port: int) -> SyncXCPClient:
    return SyncXCPClient(SyncUDP.connect(host, port))


def load_example(synchronous: bool, name: str):
    import importlib.util

    path = ROOT / "python/examples"
    if synchronous:
        path /= "sync"
    spec = importlib.util.spec_from_file_location("xcp_example", path / (name + ".py"))
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize("synchronous", [False, True])
@pytest.mark.parametrize("max_cto,ag", [(192, 1), (64, 4)])
async def test_ethernet_example_uploads_from_supplied_address(
    tmp_path: Path, synchronous: bool, max_cto: int, ag: int, monkeypatch
) -> None:
    start = 0x800A_0000
    async with ecu(
        tmp_path, "udp", "--case", "upload", "--base", hex(start), "--max-cto", str(max_cto), "--ag", str(ag)
    ) as (ready, process, stats):
        example = load_example(synchronous, "xcp_on_ethernet")
        monkeypatch.setattr(
            sys,
            "argv",
            [
                "xcp_on_ethernet.py",
                "--host",
                "127.0.0.1",
                "--port",
                str(ready["port"]),
                "--bind",
                "127.0.0.2",
                "--bind-port",
                "51010",
                "--address",
                hex(start),
            ],
        )
        if synchronous:
            await asyncio.to_thread(example.main)
        else:
            await example.main()
        await asyncio.wait_for(process.wait(), 2)
        report = json.loads(stats.read_text())
        assert set(report["commands"]) == {"ff", "f6", "f5"}
        assert report["peers"] == [["127.0.0.2", 51010]]
        chunk_size = max_cto - ag
        offset = 0
        for request in report["uploads"]:
            assert request["address"] == start + offset // ag
            size = request["elements"] * ag
            assert size == min(chunk_size, 4096 - offset)
            offset += size
        assert offset == 4096
        assert report["commands"]["f5"] == (4096 + chunk_size - 1) // chunk_size


@pytest.mark.vcan
@pytest.mark.parametrize("synchronous", [False, True])
async def test_can_examples_send_ten_startup_wakeups_and_upload(tmp_path: Path, synchronous: bool, monkeypatch) -> None:
    from automotive.sync.adapters import SocketCanAdapter as SyncCan

    async with ecu(tmp_path, "can", "--require-wakeup", "--case", "upload") as (_, process, stats):
        example = load_example(synchronous, "xcp_on_can")
        monkeypatch.setattr(
            example, "CanAdapter", lambda: SyncCan("vcan0") if synchronous else SocketCanAdapter("vcan0")
        )
        if synchronous:
            await asyncio.to_thread(example.main)
        else:
            await example.main()
        await asyncio.wait_for(process.wait(), 2)
        report = json.loads(stats.read_text())["commands"]
        assert report["wake541"] == 10
        assert set(report) == {"wake541", "ff", "f6", "f5"}
