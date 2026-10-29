"""Scapy DoIP ECU tests over ordinary loopback sockets, including on Windows."""

from __future__ import annotations

import asyncio
import sys
import tempfile
from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from dataclasses import dataclass

import pytest

from automotive.errors import DoIPError
from automotive.protocols.uds import UDSClient, UDSConfig
from automotive.sync.protocols.uds import UDSClient as SyncUDSClient
from automotive.sync.transports.doip import DoIP as SyncDoIP
from automotive.sync.transports.doip import discover as sync_discover
from automotive.transports.doip import Discovery, DiscoveryConfig, DoIP, DoIPConfig, discover

from .conftest import ROOT
from .uds_services import exercise_services


@dataclass
class Ecu:
    host: str
    port: int

    @property
    def discovery(self) -> DiscoveryConfig:
        host = f"[{self.host}]" if ":" in self.host else self.host
        return DiscoveryConfig(bind_address=f"{host}:0", destination=f"{host}:{self.port}", timeout=0.1)


@asynccontextmanager
async def ecu(host: str = "127.0.0.1", case: str = "normal", announce_to: int | None = None) -> AsyncGenerator[Ecu]:
    args = [sys.executable, str(ROOT / "scripts/vecu_doip.py"), "--host", host, "--case", case]
    if announce_to is not None:
        args += ["--announce-to", str(announce_to)]
    with tempfile.TemporaryFile() as stderr:
        process = await asyncio.create_subprocess_exec(*args, stdout=asyncio.subprocess.PIPE, stderr=stderr, cwd=ROOT)
        try:
            assert process.stdout is not None
            ready = (await asyncio.wait_for(process.stdout.readline(), 10)).decode().strip()
            assert ready.startswith("READY "), "Scapy ECU did not start"
            yield Ecu(host, int(ready.removeprefix("READY ")))
        except BaseException:
            stderr.seek(0)
            print(stderr.read().decode(errors="replace"), file=sys.stderr)
            raise
        finally:
            if process.returncode is None:
                process.terminate()
            await asyncio.wait_for(process.wait(), 5)


@pytest.mark.parametrize("host", ["127.0.0.1", "::1"])
async def test_discover_connect_and_all_uds_services(host: str) -> None:
    async with ecu(host) as server:
        vehicles = await discover(server.discovery)
        assert len(vehicles) == 1
        vehicle = vehicles[0]
        assert vehicle.host == host
        assert vehicle.vin == b"WVWZZZ1JZXW000001"
        async with await vehicle.connect(port=server.port) as transport:
            uds = UDSClient(transport, UDSConfig(p2_timeout=1, request_timeout=5))
            await exercise_services(uds)
            with uds.suppress_positive_responses():
                await uds.tester_present()
            await uds.tester_present()


@pytest.mark.parametrize("host", ["127.0.0.1", "::1"])
async def test_sync_discovery_and_uds(host: str) -> None:
    async with ecu(host) as server:

        def exchange() -> None:
            vehicles = sync_discover(server.discovery)
            assert len(vehicles) == 1
            with vehicles[0].connect(port=server.port) as transport:
                uds = SyncUDSClient(transport, UDSConfig(p2_timeout=1, request_timeout=5))
                uds.tester_present()
                assert uds.read_data_by_identifier(0x1234) == b"deadbeef"
                uds.write_data_by_identifier(0x1234, b"updated!")
                assert uds.read_data_by_identifier(0x1234) == b"updated!"
                assert uds.request_download_alfid(0, 0, 0x123456, 8, 3, 2) == 10
                assert uds.transfer_data(1, b"download") is None
                assert uds.request_transfer_exit() is None
                assert uds.request_upload_alfid(0, 0, 0x123456, 8, 3, 2) == 10
                assert uds.transfer_data(1) == b"download"
                assert uds.request_transfer_exit() is None
            with SyncDoIP.connect(host, 0x1000, port=server.port) as transport:
                assert SyncUDSClient(transport).read_data_by_identifier(0x1234) == b"deadbeef"

        await asyncio.to_thread(exchange)


@pytest.mark.parametrize("host", ["127.0.0.1", "::1"])
async def test_passive_announcements_and_explicit_trigger(host: str) -> None:
    endpoint = f"[{host}]" if ":" in host else host
    config = DiscoveryConfig(bind_address=f"{endpoint}:0", destination=f"{endpoint}:13400")
    async with await Discovery.listen(config) as listener:
        port = int(listener.local_address.rpartition(":")[2])
        assert port == 13400
        async with ecu(host, announce_to=port) as server:
            announcement = await asyncio.wait_for(listener.recv(), 2)
            assert announcement.logical_address == 0x1000
            async with await Discovery.bind(server.discovery) as requests:
                await requests.request()
                assert (await asyncio.wait_for(requests.recv(), 2)).eid == announcement.eid
    assert listener.closed
    # Explicit close must release port 13400 while the Python object still exists.
    async with await Discovery.listen(config):
        pass


async def test_raw_transport_and_error_codes() -> None:
    async with (
        ecu(case="fragmented") as server,
        await DoIP.connect(server.host, 0x1000, port=server.port, config=DoIPConfig(source_address=0xE80)) as transport,
        transport.recv() as received,
    ):
        await transport.send(b"\x22\x12\x34")
        assert await asyncio.wait_for(anext(received), 2) == b"\x62\x12\x34deadbeef"
    for case, code in [("nack", 3), ("activation-denied", 6)]:
        async with ecu(case=case) as server:
            with pytest.raises(DoIPError) as error:
                async with await DoIP.connect(server.host, 0x1000, port=server.port) as transport:
                    await transport.send(b"\x3e\x00")
            assert error.value.code == code


async def test_close_wakes_passive_receive() -> None:
    async with await Discovery.bind() as listener:
        receiving = asyncio.create_task(listener.recv())
        await asyncio.sleep(0)
        await listener.aclose()
        with pytest.raises(ConnectionError):
            await asyncio.wait_for(receiving, 1)
