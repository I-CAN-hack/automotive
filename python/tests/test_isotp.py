"""ISO-TP wire exchanges against the existing Scapy echo ECU."""

import asyncio
import time

import pytest

from automotive.adapters import Adapter, SocketCanConfig, get_adapter
from automotive.can import Frame, Identifier
from automotive.transports.isotp import IsoTPConfig

from .conftest import ECU, EcuFactory

pytestmark = pytest.mark.vcan


def assert_blocks(frames: list[Frame], tx: Identifier, rx: Identifier, bs: int, offset: int) -> None:
    segmented, count, controls = False, 0, 0
    for frame in frames:
        data = frame.data[offset:]
        if not data:
            continue
        if frame.id == tx and data[0] & 0xF0 == 0x10:
            segmented, count, controls = True, 0, 0
        elif segmented and frame.id == rx and data[0] & 0xF0 == 0x30:
            assert data[:2] == bytes((0x30, bs))
            if controls:
                assert bs > 0 and count == bs
            controls += 1
            count = 0
        elif segmented and frame.id == tx and data[0] & 0xF0 == 0x20:
            count += 1
            assert bs == 0 or count <= bs


@pytest.mark.parametrize(
    "fd,padding,address,max_dlen,bs,receive_bs",
    [
        (False, None, None, 8, 0, 0),
        (False, 0xAA, None, 8, 3, 2),
        (False, 0xCC, 0xDA, 8, 3, 2),
        (True, None, None, 64, 0, 0),
        (True, 0xAA, 0xDA, 32, 2, 3),
        (True, None, None, 8, 2, 2),
    ],
)
async def test_echo_and_flow_control(
    ecu_factory: EcuFactory, fd: bool, padding: int | None, address: int | None, max_dlen: int, bs: int, receive_bs: int
) -> None:
    config = IsoTPConfig(
        tx_id=0x7A1,
        fd=fd,
        padding=padding,
        ext_address=address,
        max_dlen=max_dlen,
        block_size=receive_bs,
        receive_separation_time_min=0.001,
        timeout=2.0,
    )
    async with (
        ecu_factory(script="vecu_isotp.py", config=config, stmin=1, bs=bs) as ecu,
        await get_adapter(SocketCanConfig(interface="vcan0")) as monitor,
    ):
        await _echo_and_capture(ecu, monitor, fd, bs, receive_bs, address)


async def _echo_and_capture(
    ecu: ECU, monitor: Adapter, fd: bool, bs: int, receive_bs: int, address: int | None
) -> None:
    # A separate socket sees wire order; the transmitting socket's emulated
    # confirmation echoes may be processed after an incoming flow control.
    async with ecu.transport.recv() as messages, monitor.recv() as capture:
        for length in (1, 7, 8, 32, 129, 512):
            payload = bytes(i % 256 for i in range(length))
            start = time.monotonic()
            await ecu.transport.send(memoryview(bytearray(payload)))
            assert await asyncio.wait_for(anext(messages), 5) == payload
            if not fd and length == 512:
                assert time.monotonic() - start >= 0.05  # Peer's 1 ms STmin applies to 70+ CFs.
        marker = Frame(id=0x123)
        await ecu.adapter.send(marker)
        frames = []
        async with asyncio.timeout(2):
            async for frame in capture:
                if frame.id == marker.id:
                    break
                frames.append(frame)
        offset = int(address is not None)
        raw_tx = int(ecu.transport.config.tx_id)
        tx = Identifier(raw_tx, raw_tx > 0x7FF)
        rx = ecu.transport.config.resolved_rx_id
        assert_blocks(frames, tx, rx, bs, offset)
        assert_blocks(frames, rx, tx, receive_bs, offset)


async def test_default_configuration_and_subscription_before_send(ecu_factory: EcuFactory) -> None:
    async with (
        ecu_factory(script="vecu_isotp.py", config=IsoTPConfig(tx_id=0x7A1)) as ecu,
        ecu.transport.recv() as messages,
    ):
        await ecu.transport.send(b"fast")
        # The peer may reply before anext starts: the receiver is already subscribed.
        await asyncio.sleep(0.02)
        assert await anext(messages) == b"fast"


async def test_extended_identifiers_with_derived_receive_id(ecu_factory: EcuFactory) -> None:
    config = IsoTPConfig(tx_id=Identifier.extended(0x18DA10F1), fd=True, max_dlen=32)
    async with ecu_factory(script="vecu_isotp.py", config=config) as ecu, ecu.transport.recv() as messages:
        payload = bytes(i % 256 for i in range(257))
        await ecu.transport.send(payload)
        assert await asyncio.wait_for(anext(messages), 3) == payload
