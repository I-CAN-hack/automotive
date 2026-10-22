"""Exchange complete ISO-TP messages with an echo ECU at transmit ID 0x7A1."""

from automotive.sync.adapters import Adapter
from automotive.sync.transports.isotp import IsoTPAdapter


def main() -> None:
    """Subscribe before sending so a fast response is retained."""
    adapter = Adapter()
    transport = IsoTPAdapter.from_id(adapter, 0x7A1)
    messages = transport.recv()
    transport.send(b"Hello from Python")
    print(next(messages))


if __name__ == "__main__":
    main()
