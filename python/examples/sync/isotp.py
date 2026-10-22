"""Exchange complete ISO-TP messages with an echo ECU at transmit ID 0x7A1."""

from automotive.sync import Adapter, IsoTPAdapter


def main() -> None:
    """Subscribe before sending so a fast response is retained."""
    with (
        Adapter() as adapter,
        IsoTPAdapter.from_id(adapter, 0x7A1) as transport,
        transport.recv() as messages,
    ):
        transport.send(b"Hello from Python")
        print(next(messages))


if __name__ == "__main__":
    main()
