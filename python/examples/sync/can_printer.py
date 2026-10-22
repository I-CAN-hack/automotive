"""Print received CAN frames using an ordinary iterator."""

from automotive.sync import Adapter


def main() -> None:
    """Discover an adapter and receive frames until interrupted with Ctrl+C."""
    adapter = Adapter()
    for frame in adapter.recv():
        print(frame)


if __name__ == "__main__":
    main()
