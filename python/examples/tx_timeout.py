"""Queue two CAN frames and retry after a transmit timeout/queue cancellation.

Use PEAK or Vector with an ACKing peer absent to observe timeout recovery.
Other adapters may wait indefinitely because they do not support TX deadlines.
"""

import asyncio

from automotive.adapters import get_adapter
from automotive.can import Frame
from automotive.errors import TransmissionCancelledError


async def main() -> None:
    """Keep the same adapter open and report both concurrent send results."""
    async with await get_adapter() as adapter:
        counter = 0
        while True:
            first = Frame(id=0x123, data=counter.to_bytes(8, "little"))
            second = Frame(id=0x124, data=counter.to_bytes(8, "little"))

            async def delayed_send(frame: Frame) -> None:
                await asyncio.sleep(0.25)
                await adapter.send(frame)

            results = await asyncio.gather(adapter.send(first), delayed_send(second), return_exceptions=True)
            retry = False
            for frame, result in zip((first, second), results):
                if isinstance(result, (TimeoutError, TransmissionCancelledError)):
                    retry = True
                    print(frame.id, counter, type(result).__name__)
                elif isinstance(result, BaseException):
                    raise result
                else:
                    print(frame.id, counter, "TX confirmed")
            if not retry:
                return
            counter = (counter + 1) & 0xFFFFFFFFFFFFFFFF
            print("Keeping the adapter open; retrying both frames in one second...")
            await asyncio.sleep(1)


if __name__ == "__main__":
    asyncio.run(main())
