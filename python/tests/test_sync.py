"""Exercise the synchronous façade through real CAN frames and the Scapy ECU."""

import asyncio
from concurrent.futures import ThreadPoolExecutor

import pytest

from automotive.sync.adapters import SocketCanAdapter
from automotive.sync.can import Frame, Identifier
from automotive.sync.protocols.uds import SessionType, UDSClient, UDSConfig
from automotive.sync.transports.isotp import IsoTP, IsoTPConfig

from .conftest import EcuFactory

pytestmark = pytest.mark.vcan


async def test_can_frame_round_trip() -> None:
    frame = Frame(id=Identifier.extended(0x123), data=bytes(range(12)), fd=True, brs=True)
    sender = SocketCanAdapter("vcan0")
    try:
        with SocketCanAdapter("vcan0") as monitor, monitor.recv() as frames:
            await asyncio.to_thread(sender.send, frame)
            received = await asyncio.wait_for(asyncio.to_thread(lambda: next(frames)), 2)
            assert received.id == 0x123
            assert received.id is not None and 0x100 <= received.id < 0x200
            assert received == frame
    finally:
        sender.close()


@pytest.mark.parametrize("fd,brs", [(False, None), (True, None), (True, False), (True, True)])
@pytest.mark.parametrize("batch_send", [True, False])
async def test_isotp_segmented_echo_and_receive_timeout(
    ecu_factory: EcuFactory, fd: bool, brs: bool | None, batch_send: bool
) -> None:
    config = IsoTPConfig(tx_id=0x7A1, fd=fd, brs=brs, batch_send=batch_send, block_size=2, timeout=2.0)
    async with ecu_factory(script="vecu_isotp.py", config=config, bs=2):

        def exchange() -> None:
            with (
                SocketCanAdapter("vcan0") as adapter,
                IsoTP(adapter, config) as transport,
                transport.recv() as messages,
                adapter.recv() as frames,
            ):
                with pytest.raises(TimeoutError):
                    next(messages)
                payload = bytes(range(256)) * 2
                transport.send(memoryview(payload))
                assert next(messages) == payload
                adapter.send(Frame(id=0x123))
                for frame in frames:
                    if frame.id == 0x123:
                        break
                    if frame.id == config.tx_id:
                        assert frame.fd is fd
                        assert frame.brs is (fd and brs is not False)

        await asyncio.to_thread(exchange)


async def test_suppression_is_nested_and_thread_local_on_the_wire(ecu_factory: EcuFactory) -> None:
    async with ecu_factory():

        def exchange() -> None:
            with (
                SocketCanAdapter("vcan0") as adapter,
                IsoTP.from_id(adapter, 0x7A1) as transport,
                adapter.recv() as frames,
            ):
                uds = UDSClient(transport, UDSConfig(p2_timeout=0.1, use_server_timing=False))
                with uds.suppress_positive_responses() as suppressed:
                    suppressed.tester_present()
                    with pytest.raises(LookupError), uds.suppress_positive_responses():
                        uds.tester_present()
                        raise LookupError("restore suppression after an exception")
                    with ThreadPoolExecutor(max_workers=1) as workers:
                        workers.submit(uds.tester_present).result(timeout=2)
                    uds.tester_present()
                uds.tester_present()
                sub_functions = []
                for frame in frames:
                    if frame.id == Identifier.standard(0x7A1) and frame.data[1:2] == b"\x3e":
                        sub_functions.append(frame.data[2])
                    if len(sub_functions) == 5:
                        break
                assert sub_functions == [0x80, 0x80, 0, 0x80, 0]

        await asyncio.to_thread(exchange)


async def test_session_timing_survives_transport_idle_timeouts(ecu_factory: EcuFactory) -> None:
    async with ecu_factory(case="repeated-pending"):

        def exchange() -> None:
            with (
                SocketCanAdapter("vcan0") as adapter,
                IsoTP(adapter, IsoTPConfig(tx_id=0x7A1, timeout=0.04)) as transport,
            ):
                uds = UDSClient(transport, UDSConfig(p2_timeout=0.1, request_timeout=2.0))
                timing = uds.diagnostic_session_control(SessionType.EXTENDED_DIAGNOSTIC)
                assert timing is not None and timing.p2_star_server_max == 5.0
                # Each pending-response gap is 75 ms, exceeding the transport's 40 ms.
                with uds.suppress_positive_responses():
                    uds.tester_present()

        await asyncio.to_thread(exchange)


async def test_parent_close_interrupts_synchronous_uds(ecu_factory: EcuFactory) -> None:
    async with ecu_factory(case="silent") as ecu, ecu.adapter.recv() as frames:
        with SocketCanAdapter("vcan0") as adapter, IsoTP.from_id(adapter, 0x7A1) as transport:
            uds = UDSClient(transport, UDSConfig(p2_timeout=5.0))
            pending = asyncio.create_task(asyncio.to_thread(uds.tester_present))
            try:
                async with asyncio.timeout(2):
                    while (await anext(frames)).id != Identifier.standard(0x7A1):
                        pass
            finally:
                await asyncio.to_thread(adapter.close)
                with pytest.raises(ConnectionError):
                    await asyncio.wait_for(pending, 1)


async def test_client_retains_adapter_after_python_owners_are_released(ecu_factory: EcuFactory) -> None:
    async with ecu_factory():

        def exchange() -> None:
            adapter = SocketCanAdapter("vcan0")
            transport = IsoTP.from_id(adapter, 0x7A1)
            uds = UDSClient(transport, UDSConfig(p2_timeout=1.0))
            del adapter, transport
            uds.tester_present()
            assert uds.read_data_by_identifier(0x1234) == b"deadbeef"

        await asyncio.to_thread(exchange)
