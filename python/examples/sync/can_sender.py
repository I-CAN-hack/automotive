"""Send one Classic CAN frame through the first available adapter."""

from automotive.sync.adapters import CanAdapter
from automotive.sync.can import Frame


def main() -> None:
    """Discover an adapter and send a frame."""
    adapter = CanAdapter()
    frame = Frame(id=0x123, data=b"\x01\x02\x03\x04")
    adapter.send(frame)
    print("Sent", frame)


if __name__ == "__main__":
    main()
