"""Blocking transports exchanging complete diagnostic protocol data units."""

from ..._base import Buffer
from ...transports import Transport as _AsyncTransport
from .._base import Receiver, Resource
from .._runtime import call, run


class Transport(Resource[_AsyncTransport]):
    """A blocking message transport usable by UDS, independent of its protocol.

    Concrete transports handle framing, addressing, and connection setup.
    Closing a transport invalidates its receivers and diagnostic clients.
    """

    def send(self, data: Buffer) -> None:
        """Transmit one complete PDU, blocking until the transport completes it.

        Args:
            data: Bytes, bytearray, or contiguous byte-oriented memoryview.

        Transport failures raise the corresponding protocol or connection error.
        A transmission deadline raises TimeoutError.
        """
        run(self._async.send, data)

    def recv(self) -> Receiver[bytes]:
        """Subscribe immediately to complete received PDUs as bytes.

        Returns:
            A blocking iterator supporting with and close(). Subscribe before
            sending. A receive deadline raises TimeoutError; next() can resume.
        """
        return Receiver(call(self._async.recv))


__all__ = ["Transport"]
