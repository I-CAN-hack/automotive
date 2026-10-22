"""Configure a PEAK PCAN-USB FD adapter for 500 kbit/s CAN and 2 Mbit/s CAN FD."""

from automotive.can.bitrate import BitrateSettings
from automotive.sync import PeakAdapter, PeakConfig


def main() -> None:
    """Configure nominal and data-phase timing, then print received frames."""
    config = PeakConfig(
        bitrate=BitrateSettings(
            bitrate=500_000,
            sample_point=0.8,
            data_bitrate=2_000_000,
            data_sample_point=0.8,
        ),
    )
    print("Resolved timing:", config.resolved_bitrate)
    adapter = PeakAdapter(config)
    for frame in adapter.recv():
        print(frame)


if __name__ == "__main__":
    main()
