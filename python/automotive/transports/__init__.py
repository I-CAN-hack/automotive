"""Transports exchanging complete diagnostic protocol data units."""

from .. import _native
from .._base import AsyncResource, Buffer, Receiver, buffer_bytes


class Transport(AsyncResource[_native._Transport]):
    """A message transport usable by UDS, independent of the underlying protocol.

    Concrete transports handle framing, addressing, and connection setup.
    Closing a transport invalidates its receivers and diagnostic clients.
    """

    async def send(self, data: Buffer) -> None:
        """Transmit one complete PDU using the transport's framing and limits.

        Args:
            data: Bytes, bytearray, or contiguous byte-oriented memoryview.

        Transport failures raise the corresponding protocol or connection error.
        A transmission deadline raises TimeoutError.
        """
        await self._native.send(buffer_bytes(data))

    def recv(self) -> Receiver[bytes]:
        """Subscribe immediately to complete received PDUs as bytes.

        Returns:
            An async iterator with aclose and async context management.

        Subscribe before sending. Closing this receiver releases its subscription.
        A receive deadline raises TimeoutError; another anext may continue.
        """
        return Receiver(self._native.recv(), lambda value: value)


__all__ = ["Transport"]
