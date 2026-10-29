"""Shared UDS service exchanges against Scapy over CAN and DoIP."""

import pytest

from automotive.errors import NegativeResponseError
from automotive.protocols.uds import DTCFormatIdentifier, RoutineControlType, SessionType, UDSClient


async def exercise_services(uds: UDSClient) -> None:
    await uds.tester_present()
    with pytest.raises(NegativeResponseError) as rejected:
        await uds.diagnostic_session_control(2)
    assert rejected.value.code == 0x33
    timing = await uds.diagnostic_session_control(SessionType.EXTENDED_DIAGNOSTIC)
    assert timing is not None
    assert timing.p2_server_max == 1.0
    assert timing.p2_star_server_max == 5.0
    assert await uds.ecu_reset(1) is None
    assert await uds.ecu_reset(4) == 2
    assert await uds.security_access(1) == b"seed"
    with pytest.raises(NegativeResponseError) as bad_key:
        await uds.security_access(2, b"bad!")
    assert bad_key.value.code == 0x35
    assert await uds.security_access(2, memoryview(b"key!")) is None

    assert await uds.read_data_by_identifier(0x1234) == b"deadbeef"
    await uds.write_data_by_identifier(0x1234, bytearray(b"updated!"))
    assert await uds.read_data_by_identifier(0x1234) == b"updated!"
    assert await uds.request(0x22, data=b"\x12\x34") == b"\x12\x34updated!"

    report = await uds.read_dtc_information_number_of_dtc_by_status_mask(0xFF)
    assert report is not None
    assert report.dtc_status_availability_mask == 0xFF
    assert report.dtc_format_identifier == DTCFormatIdentifier.ISO_14229_1_DTC_FORMAT
    assert report.dtc_count == 2

    address, size = 0x123456, 8
    assert await uds.read_memory_by_address_alfid(address, size, 3, 2) == b"abcdefgh"
    await uds.write_memory_by_address_alfid(address, size, 3, 2, b"written!")
    assert await uds.read_memory_by_address(b"\x12\x34\x56", b"\x00\x08") == b"written!"
    await uds.write_memory_by_address(b"\x12\x34\x56", b"\x00\x08", b"restored")
    assert await uds.read_memory_by_address_alfid(address, size, 3, 2) == b"restored"

    assert await uds.routine_control(RoutineControlType.START, 0x1234) == b"\x00"
    assert await uds.routine_control(RoutineControlType.STOP, 0x1234) is None
    assert await uds.routine_control(RoutineControlType.REQUEST_RESULTS, 0x1234) == b"done"

    assert await uds.request_download_alfid(0, 0, address, size, 3, 2) == 10
    assert await uds.transfer_data(1, bytearray(b"download")) is None
    assert await uds.request_transfer_exit() is None
    assert await uds.read_memory_by_address_alfid(address, size, 3, 2) == b"download"
    assert await uds.request_upload_alfid(0, 0, address, size, 3, 2) == 10
    assert await uds.transfer_data(1) == b"download"
    assert await uds.request_transfer_exit(memoryview(b"\xaa\x55")) == b"\xaa\x55"
