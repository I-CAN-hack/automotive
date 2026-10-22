"""Send one Classic CAN frame without asyncio."""

from automotive.sync.adapters import Adapter
from automotive.sync.can import Frame


def main() -> None:
    """Discover an adapter and send a frame."""
    adapter = Adapter()
    frame = Frame(id=0x123, data=b"\x01\x02\x03\x04")
    adapter.send(frame)
    print("Sent", frame)


if __name__ == "__main__":
    main()
