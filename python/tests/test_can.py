"""Verify Python frame conversion through real SocketCAN wire frames."""

import asyncio

import pytest

from automotive.adapters import SocketCanAdapter, SocketCanConfig
from automotive.can import Frame, Identifier

pytestmark = pytest.mark.vcan


@pytest.mark.parametrize(
    "frame",
    [
        Frame(id=0x123, data=bytearray(b"classic")),
        Frame(id=Identifier.extended(0x123), data=b"extended"),
        Frame(id=0x345, data=bytes(range(12)), fd=True, brs=True),
        Frame.remote(0x321, dlc=8),
    ],
    ids=["classic", "extended-low-id", "fd-brs", "remote"],
)
async def test_frame_round_trip(frame: Frame) -> None:
    config = SocketCanConfig(interface="vcan0")
    sender = SocketCanAdapter(config)
    try:
        async with SocketCanAdapter("vcan0") as monitor, monitor.recv() as frames:
            await sender.send(frame)
            received = await asyncio.wait_for(anext(frames), 1)
            assert received == frame
    finally:
        await sender.aclose()
