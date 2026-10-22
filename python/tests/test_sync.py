"""Exercise the synchronous façade through real CAN frames and the Scapy ECU."""

import asyncio
from concurrent.futures import ThreadPoolExecutor

import pytest

from automotive.can import Frame, Identifier
from automotive.errors import NegativeResponseError
from automotive.protocols.uds import DTCFormatIdentifier, RoutineControlType, SessionType
from automotive.sync import IsoTPAdapter, IsoTPConfig, SocketCanAdapter, UDSClient, UDSConfig

from .conftest import EcuFactory

pytestmark = pytest.mark.vcan


async def test_can_frame_round_trip() -> None:
    frame = Frame(id=Identifier.extended(0x123), data=bytes(range(12)), fd=True, brs=True)
    with SocketCanAdapter("vcan0") as sender, SocketCanAdapter("vcan0") as monitor, monitor.recv() as frames:
        await asyncio.to_thread(sender.send, frame)
        assert await asyncio.wait_for(asyncio.to_thread(next, frames), 2) == frame


@pytest.mark.parametrize("fd", [False, True])
async def test_isotp_segmented_echo_and_receive_timeout(ecu_factory: EcuFactory, fd: bool) -> None:
    config = IsoTPConfig(tx_id=0x7A1, fd=fd, block_size=2, timeout=2.0)
    async with ecu_factory(script="vecu_isotp.py", config=config, bs=2):

        def exchange() -> None:
            with (
                SocketCanAdapter("vcan0") as adapter,
                IsoTPAdapter(adapter, config) as transport,
                transport.recv() as messages,
            ):
                with pytest.raises(TimeoutError):
                    next(messages)
                payload = bytes(range(256)) * 2
                transport.send(memoryview(payload))
                assert next(messages) == payload

        await asyncio.to_thread(exchange)


async def test_all_implemented_uds_service_ids(ecu_factory: EcuFactory) -> None:
    async with ecu_factory():

        def exchange() -> None:
            with SocketCanAdapter("vcan0") as adapter, IsoTPAdapter.from_id(adapter, 0x7A1) as transport:
                uds = UDSClient(transport, UDSConfig(p2_timeout=1.0, request_timeout=5.0))
                uds.tester_present()
                with pytest.raises(NegativeResponseError) as rejected:
                    uds.diagnostic_session_control(2)
                assert rejected.value.code == 0x33
                timing = uds.diagnostic_session_control(SessionType.EXTENDED_DIAGNOSTIC)
                assert timing is not None and timing.p2_server_max == 1.0
                assert uds.ecu_reset(4) == 2
                assert uds.security_access(1) == b"seed"
                assert uds.security_access(2, memoryview(b"key!")) is None
                assert uds.read_data_by_identifier(0x1234) == b"deadbeef"
                uds.write_data_by_identifier(0x1234, bytearray(b"updated!"))
                assert uds.request(0x22, data=b"\x12\x34") == b"\x12\x34updated!"
                report = uds.read_dtc_information_number_of_dtc_by_status_mask(0xFF)
                assert report is not None
                assert report.dtc_format_identifier == DTCFormatIdentifier.ISO_14229_1_DTC_FORMAT
                assert report.dtc_count == 2
                assert uds.read_memory_by_address_alfid(0x123456, 8, 3, 2) == b"abcdefgh"
                uds.write_memory_by_address_alfid(0x123456, 8, 3, 2, b"written!")
                assert uds.read_memory_by_address(b"\x12\x34\x56", b"\x00\x08") == b"written!"
                uds.write_memory_by_address(b"\x12\x34\x56", b"\x00\x08", b"restored")
                assert uds.routine_control(RoutineControlType.START, 0x1234) == b"\x00"
                assert uds.routine_control(RoutineControlType.REQUEST_RESULTS, 0x1234) == b"done"

                for integer_fields in (True, False):
                    if integer_fields:
                        assert uds.request_download_alfid(0, 0, 0x123456, 8, 3, 2) == 10
                    else:
                        assert uds.request_download(0, 0, b"\x12\x34\x56", b"\x00\x08") == 10
                    uds.transfer_data(1, bytearray(b"download"))
                    uds.request_transfer_exit()
                    if integer_fields:
                        assert uds.request_upload_alfid(0, 0, 0x123456, 8, 3, 2) == 10
                    else:
                        assert uds.request_upload(0, 0, b"\x12\x34\x56", b"\x00\x08") == 10
                    assert uds.transfer_data(1) == b"download"
                    uds.request_transfer_exit()

        await asyncio.to_thread(exchange)


async def test_suppression_is_nested_and_thread_local_on_the_wire(ecu_factory: EcuFactory) -> None:
    async with ecu_factory():

        def exchange() -> None:
            with (
                SocketCanAdapter("vcan0") as adapter,
                IsoTPAdapter.from_id(adapter, 0x7A1) as transport,
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


@pytest.mark.parametrize("case", ["pending-positive", "pending-negative", "pending-only"])
async def test_suppressed_response_pending(ecu_factory: EcuFactory, case: str) -> None:
    async with ecu_factory(case=case):

        def exchange() -> None:
            with SocketCanAdapter("vcan0") as adapter, IsoTPAdapter.from_id(adapter, 0x7A1) as transport:
                uds = UDSClient(
                    transport,
                    UDSConfig(p2_timeout=0.1, p2_star_timeout=0.2, request_timeout=2.0, use_server_timing=False),
                )
                with uds.suppress_positive_responses():
                    if case == "pending-positive":
                        uds.tester_present()
                    elif case == "pending-negative":
                        with pytest.raises(NegativeResponseError) as error:
                            uds.tester_present()
                        assert error.value.code == 0x33
                    else:
                        with pytest.raises(TimeoutError):
                            uds.tester_present()

        await asyncio.to_thread(exchange)


async def test_parent_close_interrupts_synchronous_uds(ecu_factory: EcuFactory) -> None:
    async with ecu_factory(case="silent") as ecu, ecu.adapter.recv() as frames:
        with SocketCanAdapter("vcan0") as adapter, IsoTPAdapter.from_id(adapter, 0x7A1) as transport:
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
