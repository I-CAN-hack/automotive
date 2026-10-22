"""Print received CAN frames using an ordinary iterator."""

from automotive.sync import Adapter


def main() -> None:
    """Discover an adapter and receive frames until interrupted with Ctrl+C."""
    with Adapter() as adapter, adapter.recv() as frames:
        for frame in frames:
            print(frame)


if __name__ == "__main__":
    main()
