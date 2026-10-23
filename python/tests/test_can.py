"""Verify Python frame conversion through real SocketCAN wire frames."""

import asyncio

import pytest

from automotive.adapters import SocketCanAdapter, SocketCanConfig
from automotive.can import Frame, Identifier

pytestmark = pytest.mark.vcan


@pytest.mark.parametrize(
    "frame,rx_addr",
    [
        (Frame(id=0x123, data=bytearray(b"classic")), 0x123),
        (Frame(id=Identifier.extended(0x123), data=b"extended"), 0x123),
        (Frame(id=0x345, data=bytes(range(12)), fd=True, brs=True), 0x345),
        (Frame.remote(0x321, dlc=8), 0x321),
    ],
    ids=["classic", "extended-low-id", "fd-brs", "remote"],
)
async def test_frame_round_trip(frame: Frame, rx_addr: int) -> None:
    config = SocketCanConfig(interface="vcan0")
    sender = SocketCanAdapter(config)
    try:
        async with SocketCanAdapter("vcan0") as monitor, monitor.recv() as frames:
            await sender.send(frame)
            received = await asyncio.wait_for(anext(frames), 1)
            assert received.id == rx_addr
            assert received == frame
    finally:
        await sender.aclose()


async def test_recv_for() -> None:
    expected = [
        Frame(id=0x123, data=b"classic"),
        Frame(id=Identifier.extended(0x123), data=b"extended"),
        Frame(id=0x345, data=bytes(range(12)), fd=True, brs=True),
        Frame.remote(0x321, dlc=8),
    ]
    async with (
        SocketCanAdapter("vcan0") as sender,
        SocketCanAdapter("vcan0") as monitor,
        monitor.recv() as independent,
    ):

        async def transmit() -> None:
            await asyncio.sleep(0.01)
            for frame in expected:
                await sender.send(frame)

        received, _ = await asyncio.gather(monitor.recv_for(0.1), transmit())
        assert received == expected
        for frame in expected:
            assert await asyncio.wait_for(anext(independent), 1) == frame
        assert await monitor.recv_for(0.01) == []
