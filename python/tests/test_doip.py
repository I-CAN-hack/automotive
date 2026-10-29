"""Scapy DoIP ECU tests over ordinary loopback sockets, including on Windows."""

from __future__ import annotations

import asyncio

import pytest

from automotive.errors import DoIPError
from automotive.protocols.uds import UDSClient, UDSConfig
from automotive.sync.protocols.uds import UDSClient as SyncUDSClient
from automotive.sync.transports.doip import DoIP as SyncDoIP
from automotive.sync.transports.doip import discover as sync_discover
from automotive.transports.doip import Discovery, DiscoveryConfig, DoIP, DoIPConfig, discover

from .ecu import doip_ecu as ecu


@pytest.mark.parametrize("host", ["127.0.0.1", "::1"])
async def test_discover_and_connect(host: str) -> None:
    async with ecu(host) as server:
        vehicles = await discover(server.discovery)
        assert len(vehicles) == 1
        vehicle = vehicles[0]
        assert vehicle.host == host
        assert vehicle.vin == b"WVWZZZ1JZXW000001"
        async with await vehicle.connect(port=server.port) as transport:
            uds = UDSClient(transport, UDSConfig(p2_timeout=1, request_timeout=5))
            assert await uds.read_data_by_identifier(0x1234) == b"deadbeef"


@pytest.mark.parametrize("host", ["127.0.0.1", "::1"])
async def test_sync_discovery_and_uds(host: str) -> None:
    async with ecu(host) as server:

        def exchange() -> None:
            vehicles = sync_discover(server.discovery)
            assert len(vehicles) == 1
            with vehicles[0].connect(port=server.port) as transport:
                uds = SyncUDSClient(transport, UDSConfig(p2_timeout=1, request_timeout=5))
                assert uds.read_data_by_identifier(0x1234) == b"deadbeef"
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
