"""Scapy ECU lifecycle shared by Python protocol regression tests."""

from __future__ import annotations

import asyncio
import socket
import sys
import tempfile
from collections.abc import AsyncGenerator
from contextlib import AbstractAsyncContextManager, asynccontextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

import pytest

from automotive.adapters import CanAdapter, SocketCanAdapter
from automotive.transports.isotp import IsoTP, IsoTPConfig

ROOT = Path(__file__).resolve().parents[2]


def pytest_addoption(parser: pytest.Parser) -> None:
    parser.addoption("--run-vcan", action="store_true", help="Run Scapy ECU protocol tests on Linux vcan0")


def pytest_collection_modifyitems(config: pytest.Config, items: list[pytest.Item]) -> None:
    if not config.getoption("--run-vcan"):
        for item in items:
            if "vcan" in item.keywords:
                item.add_marker(pytest.mark.skip(reason="use --run-vcan to enable Scapy ECU tests"))


@pytest.fixture(scope="session", autouse=True)
def require_vcan(request: pytest.FixtureRequest) -> None:
    if not request.config.getoption("--run-vcan"):
        return
    if sys.platform != "linux":
        pytest.fail("--run-vcan requires Linux")
    try:
        with socket.socket(socket.PF_CAN, socket.SOCK_RAW, socket.CAN_RAW) as check:
            check.bind(("vcan0",))
        with socket.socket(socket.PF_CAN, socket.SOCK_DGRAM, socket.CAN_ISOTP):
            pass
    except OSError as error:
        pytest.fail(f"vcan0/CAN_ISOTP unavailable; run scripts/set_up_vcan.sh: {error}")


@dataclass
class ECU:
    adapter: CanAdapter
    transport: IsoTP


class EcuFactory(Protocol):
    def __call__(
        self,
        *,
        script: str = "vecu_uds.py",
        case: str | None = None,
        config: IsoTPConfig | None = None,
        stmin: int = 0,
        bs: int = 0,
    ) -> AbstractAsyncContextManager[ECU]: ...


@pytest.fixture
def ecu_factory() -> EcuFactory:
    @asynccontextmanager
    async def spawn(
        *,
        script: str = "vecu_uds.py",
        case: str | None = None,
        config: IsoTPConfig | None = None,
        stmin: int = 0,
        bs: int = 0,
    ) -> AsyncGenerator[ECU]:
        config = config or IsoTPConfig(tx_id=0x7A1, timeout=2.0)
        args = [
            sys.executable,
            str(ROOT / "scripts" / script),
            "--iface",
            "vcan0",
            "--timeout",
            "60",
            "--rx",
            str(int(config.tx_id)),
            "--tx",
            str(int(config.resolved_rx_id)),
        ]
        if case is not None:
            args += ["--suppression-case", case]
        if script == "vecu_isotp.py":
            args += ["--stmin", str(stmin), "--bs", str(bs)]
            if config.fd:
                args += ["--fd"]
            if config.padding is not None:
                args += ["--padding", str(config.padding)]
            if config.ext_address is not None:
                args += ["--ext-address", str(config.ext_address)]
        async with (
            SocketCanAdapter("vcan0") as adapter,
            IsoTP(adapter, config) as transport,
        ):
            with tempfile.TemporaryFile() as stderr:
                process: asyncio.subprocess.Process | None = None
                try:
                    async with transport.recv() as ready:
                        process = await asyncio.create_subprocess_exec(
                            *args,
                            stdout=asyncio.subprocess.DEVNULL,
                            stderr=stderr,
                            cwd=ROOT,
                        )
                        assert await asyncio.wait_for(anext(ready), 10) == b"\xaa"
                    yield ECU(adapter, transport)
                except BaseException:
                    stderr.seek(0)
                    output = stderr.read().decode(errors="replace")
                    if output:
                        print("Scapy ECU stderr:\n" + output, file=sys.stderr)
                    raise
                finally:
                    if process is not None and process.returncode is None:
                        process.terminate()
                        try:
                            await asyncio.wait_for(process.wait(), 3)
                        except TimeoutError:
                            process.kill()
                            await process.wait()

    return spawn
