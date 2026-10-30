"""CCP wire interoperability with Scapy's CRO decoder and DTO encoders."""

from __future__ import annotations

import asyncio
import json
import sys
import tempfile
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Literal

import pytest

from automotive.adapters import SocketCanAdapter
from automotive.can import Identifier
from automotive.errors import CCPResponseError
from automotive.protocols.ccp import CCPClient, CCPConfig, Resource
from automotive.sync.adapters import SocketCanAdapter as SyncAdapter
from automotive.sync.protocols.ccp import CCPClient as SyncClient

ROOT = Path(__file__).resolve().parents[2]
BASE = 0x8000_0000
MEMORY = bytes((i * 17 + 3) & 0xFF for i in range(4096))
pytestmark = pytest.mark.vcan


@asynccontextmanager
async def ecu(tmp_path: Path, byte_order: str, extended: bool):
    stats = tmp_path / "ccp-stats.json"
    options = ["--extended"] if extended else []
    with tempfile.TemporaryFile() as stderr:
        process = await asyncio.create_subprocess_exec(
            sys.executable,
            str(ROOT / "scripts/vecu_ccp.py"),
            "--byte-order",
            byte_order,
            "--stats",
            str(stats),
            *options,
            stdout=asyncio.subprocess.PIPE,
            stderr=stderr,
        )
        try:
            assert process.stdout is not None
            ready = await asyncio.wait_for(process.stdout.readline(), 10)
            assert ready, "Scapy CCP ECU exited before announcing readiness"
            yield process, stats
        finally:
            if process.returncode is None:
                process.terminate()
            await asyncio.wait_for(process.wait(), 5)
            stderr.seek(0)
            assert process.returncode in (0, -15), stderr.read().decode(errors="replace")


@pytest.mark.parametrize("byte_order,extended", [("little", False), ("big", True)])
async def test_scapy_memory_transfers_and_seed_key(
    tmp_path: Path, byte_order: Literal["little", "big"], extended: bool
) -> None:
    make_id = Identifier.extended if extended else Identifier.standard
    config = CCPConfig(tx_id=make_id(0x6EC), rx_id=make_id(0x6ED), byte_order=byte_order)
    async with ecu(tmp_path, byte_order, extended) as (process, stats):
        async with (
            SocketCanAdapter("vcan0") as adapter,
            CCPClient(adapter, config) as client,
        ):
            await client.connect(0x1234)
            assert (await client.get_ccp_version()).major == 2
            identity = await client.exchange_id()
            assert (identity.length, identity.resource_availability, identity.resource_protection) == (5, 0x43, 1)
            assert await client.upload(5) == b"SCAPY"
            await client.set_mta(BASE, 0x5A)
            with pytest.raises(CCPResponseError) as error:
                await client.download(b"x")
            assert (error.value.command, error.value.code) == (0x03, 0x35)
            seed = await client.get_seed(Resource.CAL)
            assert seed.protected and seed.data == b"seed"
            assert await client.unlock(memoryview(b"key!")) == 1
            data = bytearray()
            while len(data) < len(MEMORY):
                data.extend(await client.short_upload(BASE + len(data), min(5, len(MEMORY) - len(data)), 0x5A))
            assert data == MEMORY
            assert await client.upload(5) == MEMORY[:5]  # SHORT_UP preserved MTA0.
            await client.set_mta(BASE + 0x100, 0x5A)
            mta = await client.download(bytearray(b"hello"))
            assert (mta.address, mta.address_extension) == (BASE + 0x105, 0x5A)
            assert (await client.download6(b"world!")).address == BASE + 0x10B
            await client.set_mta(BASE + 0x100, 0x5A)
            assert await client.upload(5) + await client.upload(5) + await client.upload(1) == b"helloworld!"
            await client.disconnect()
        await asyncio.wait_for(process.wait(), 2)
        assert set(json.loads(stats.read_text())["commands"]) == {1, 2, 3, 4, 7, 0x0F, 0x12, 0x13, 0x17, 0x1B, 0x23}


async def test_scapy_sync_memory_commands(tmp_path: Path) -> None:
    def exercise() -> None:
        config = CCPConfig(tx_id=0x6EC, rx_id=0x6ED, byte_order="little")
        with (
            SyncAdapter("vcan0") as adapter,
            SyncClient(adapter, config) as client,
        ):
            client.connect(0x1234)
            assert client.get_ccp_version().minor == 1
            client.set_mta(BASE, 0x5A)
            assert client.short_upload(BASE + 10, 4, 0x5A) == MEMORY[10:14]
            assert client.upload(5) == MEMORY[:5]
            assert client.get_seed(Resource.CAL).data == b"seed"
            assert client.unlock(b"key!") == 1
            assert client.download(b"abc").address == BASE + 8
            assert client.download6(b"123456").address == BASE + 14
            client.set_mta(BASE + 5)
            assert client.upload(5) == b"abc12"
            client.disconnect()

    async with ecu(tmp_path, "little", False) as (process, _):
        await asyncio.to_thread(exercise)
        await asyncio.wait_for(process.wait(), 2)
