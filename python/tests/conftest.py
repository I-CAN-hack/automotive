"""Scapy ECU lifecycle shared by Python protocol regression tests."""

from __future__ import annotations

import asyncio
import socket
import sys
from collections.abc import AsyncGenerator, Callable, Generator
from contextlib import AbstractAsyncContextManager, AbstractContextManager, asynccontextmanager, contextmanager
from dataclasses import dataclass
from typing import Protocol

import pytest

from automotive.adapters import CanAdapter, SocketCanAdapter
from automotive.sync.adapters import SocketCanAdapter as SyncSocketCanAdapter
from automotive.sync.transports import Transport as SyncTransport
from automotive.sync.transports.doip import DoIP as SyncDoIP
from automotive.sync.transports.isotp import IsoTP as SyncIsoTP
from automotive.transports import Transport
from automotive.transports.doip import DoIP
from automotive.transports.isotp import IsoTP, IsoTPConfig

from .ecu import ROOT, doip_ecu, scapy_process


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
            ready = transport.recv()
            async with scapy_process(args):
                async with ready:
                    assert await asyncio.wait_for(anext(ready), 10) == b"\xaa"
                yield ECU(adapter, transport)

    return spawn


@dataclass
class UdsECU:
    transport: Transport
    open_sync: Callable[[], AbstractContextManager[SyncTransport]]


class UdsEcuFactory(Protocol):
    def __call__(self, *, case: str | None = None) -> AbstractAsyncContextManager[UdsECU]: ...


@pytest.fixture(
    params=[
        pytest.param("can", marks=pytest.mark.vcan),
        pytest.param("127.0.0.1", id="doip-ipv4"),
        pytest.param("::1", id="doip-ipv6"),
    ]
)
def uds_ecu_factory(request: pytest.FixtureRequest, ecu_factory: EcuFactory) -> UdsEcuFactory:
    """Run the same UDS assertions with CAN or either DoIP address family."""

    @asynccontextmanager
    async def spawn(*, case: str | None = None) -> AsyncGenerator[UdsECU]:
        if request.param == "can":

            @contextmanager
            def open_sync() -> Generator[SyncTransport]:
                with SyncSocketCanAdapter("vcan0") as adapter, SyncIsoTP.from_id(adapter, 0x7A1) as transport:
                    yield transport

            async with ecu_factory(case=case) as ecu:
                yield UdsECU(ecu.transport, open_sync)
        else:
            async with (
                doip_ecu(request.param, uds_case=case) as server,
                await DoIP.connect(server.host, 0x1000, port=server.port) as transport,
            ):
                yield UdsECU(
                    transport,
                    lambda: SyncDoIP.connect(server.host, 0x1000, port=server.port),
                )

    return spawn
