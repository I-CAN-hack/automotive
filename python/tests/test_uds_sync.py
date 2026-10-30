"""The blocking UDS service suite runs unchanged over CAN and IPv4/IPv6 DoIP."""

import asyncio

import pytest

from automotive.sync.errors import NegativeResponseError
from automotive.sync.protocols.uds import DTCFormatIdentifier, RoutineControlType, SessionType, UDSClient, UDSConfig

from .conftest import UdsEcuFactory


async def test_all_implemented_uds_service_ids(uds_ecu_factory: UdsEcuFactory) -> None:
    async with uds_ecu_factory() as ecu:

        def exchange() -> None:
            with ecu.open_sync() as transport:
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


@pytest.mark.parametrize("case", ["pending-positive", "pending-negative", "pending-only"])
async def test_suppressed_response_pending(uds_ecu_factory: UdsEcuFactory, case: str) -> None:
    async with uds_ecu_factory(case=case) as ecu:

        def exchange() -> None:
            with ecu.open_sync() as transport:
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
