"""Print frames from the first available adapter. Requires CAN hardware or vcan0."""

from automotive.sync.adapters import CanAdapter


def main() -> None:
    """Discover an adapter and print frames until interrupted."""
    adapter = CanAdapter()
    stream = adapter.recv()

    for frame in stream:
        print(frame)


if __name__ == "__main__":
    main()
