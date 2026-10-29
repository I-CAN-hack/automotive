"""End-to-end UDS behavior through the Python API and Scapy's answering machine."""

import asyncio
import time

import pytest

from automotive.adapters import SocketCanAdapter
from automotive.can import Identifier
from automotive.errors import NegativeResponseError, ProtocolDeviationError
from automotive.protocols.uds import RoutineControlType, SessionType, UDSClient, UDSConfig
from automotive.transports.isotp import IsoTP, IsoTPConfig

from .conftest import EcuFactory
from .uds_services import exercise_services

pytestmark = pytest.mark.vcan


async def test_all_implemented_service_ids(ecu_factory: EcuFactory) -> None:
    async with ecu_factory() as ecu:
        uds = UDSClient(ecu.transport, UDSConfig(p2_timeout=1.0, request_timeout=5.0))
        await exercise_services(uds)


async def test_raw_memory_fields_in_download_and_upload(ecu_factory: EcuFactory) -> None:
    # Start with a fresh answering machine: completion-response records are
    # stateful, so a previous transfer-exit echo changes subsequent matching.
    async with ecu_factory() as ecu:
        uds = UDSClient(ecu.transport, UDSConfig(p2_timeout=1.0, request_timeout=5.0))
        assert await uds.request_download(0, 0, b"\x12\x34\x56", b"\x00\x08") == 10
        await uds.transfer_data(1, b"rawbytes")
        await uds.request_transfer_exit()
        assert await uds.request_upload(0, 0, b"\x12\x34\x56", b"\x00\x08") == 10
        assert await uds.transfer_data(1) == b"rawbytes"
        await uds.request_transfer_exit()


async def test_session_timing_survives_transport_idle_timeouts(ecu_factory: EcuFactory) -> None:
    async with (
        ecu_factory(case="repeated-pending") as ecu,
        IsoTP(ecu.adapter, IsoTPConfig(tx_id=0x7A1, timeout=0.04)) as transport,
    ):
        uds = UDSClient(transport, UDSConfig(p2_timeout=0.1, request_timeout=2.0))
        timing = await uds.diagnostic_session_control(SessionType.EXTENDED_DIAGNOSTIC)
        assert timing is not None and timing.p2_star_server_max == 5.0
        # Each pending-response gap is 75 ms, exceeding the transport's 40 ms.
        with uds.suppress_positive_responses():
            await uds.tester_present()


@pytest.mark.parametrize("strict", [True, False])
async def test_suppressed_services_and_raw_outcomes(ecu_factory: EcuFactory, strict: bool) -> None:
    async with ecu_factory() as ecu:
        uds = UDSClient(ecu.transport, UDSConfig(strict=strict, p2_timeout=0.1, use_server_timing=False))
        assert await uds.request(0x3E, 0x80) is None
        with uds.suppress_positive_responses() as suppressed:
            await suppressed.tester_present()
            assert await suppressed.diagnostic_session_control(3) is None
            assert await suppressed.ecu_reset(4) is None
            assert await suppressed.security_access(1) is None
            assert await suppressed.security_access(2, b"key!") is None
            assert await suppressed.read_dtc_information_number_of_dtc_by_status_mask(0xFF) is None
            assert await suppressed.routine_control(RoutineControlType.STOP, 0x1234) is None
            assert await suppressed.routine_control(RoutineControlType.START, 0x1234) == b"\x00"
            assert await suppressed.read_data_by_identifier(0x1234) == b"deadbeef"
        assert await uds.request(0x3E, 0) == b""
        await uds.tester_present()


@pytest.mark.parametrize("case", ["pending-positive", "pending-negative", "pending-only", "repeated-pending"])
@pytest.mark.parametrize("strict", [True, False])
async def test_pending_responses(ecu_factory: EcuFactory, case: str, strict: bool) -> None:
    async with ecu_factory(case=case) as ecu:
        uds = UDSClient(
            ecu.transport,
            UDSConfig(strict=strict, p2_timeout=0.1, p2_star_timeout=0.2, request_timeout=2.0, use_server_timing=False),
        )
        start = time.monotonic()
        if case == "pending-only":
            with pytest.raises(TimeoutError):
                await uds.request(0x3E, 0x80)
        elif case == "pending-negative":
            with pytest.raises(NegativeResponseError) as error:
                await uds.request(0x3E, 0x80)
            assert error.value.code == 0x33
        else:
            assert await uds.request(0x3E, 0x80) == b""
            if case == "repeated-pending":
                assert time.monotonic() - start >= 0.2


@pytest.mark.parametrize(
    "case, expected",
    [
        ("positive", b"\x7e\x00"),
        ("bad-echo", b"\x7e\x80"),
        ("wrong-pending-sid", b"\x7f\x3f\x78"),
        ("extra-pending-data", b"\x7f\x3e\x78\xaa"),
    ],
)
async def test_strict_response_validation(ecu_factory: EcuFactory, case: str, expected: bytes) -> None:
    async with ecu_factory(case=case) as ecu:
        uds = UDSClient(ecu.transport, UDSConfig(p2_timeout=0.1, p2_star_timeout=0.2, use_server_timing=False))
        with pytest.raises(ProtocolDeviationError) as error, uds.suppress_positive_responses():
            await uds.tester_present()
        assert error.value.response == expected
        assert error.value.deviation
        permissive = UDSClient(
            ecu.transport, UDSConfig(strict=False, p2_timeout=0.1, p2_star_timeout=0.2, use_server_timing=False)
        )
        if case in {"positive", "bad-echo"}:
            with permissive.suppress_positive_responses():
                await permissive.tester_present()
        else:
            with pytest.raises(TimeoutError), permissive.suppress_positive_responses():
                await permissive.tester_present()


async def test_unknown_nrc_preserved(ecu_factory: EcuFactory) -> None:
    async with ecu_factory(case="reserved-nrc") as ecu:
        for strict in (True, False):
            uds = UDSClient(ecu.transport, UDSConfig(strict=strict, p2_timeout=0.1))
            with pytest.raises(NegativeResponseError) as error, uds.suppress_positive_responses():
                await uds.tester_present()
            assert error.value.code == 0x15


async def test_session_timing_retained_in_suppression_scope(ecu_factory: EcuFactory) -> None:
    async with ecu_factory() as ecu:
        uds = UDSClient(ecu.transport, UDSConfig(p2_timeout=0.1))
        timing = await uds.diagnostic_session_control(3)
        assert timing is not None and timing.p2_server_max == 1.0
        start = time.monotonic()
        with uds.suppress_positive_responses():
            await uds.tester_present()
        assert time.monotonic() - start >= 0.95


async def test_overall_timeout_and_required_positive_response(ecu_factory: EcuFactory) -> None:
    async with ecu_factory(case="silent") as ecu:
        uds = UDSClient(ecu.transport, UDSConfig(p2_timeout=0.1, use_server_timing=False))
        with uds.suppress_positive_responses():
            await uds.tester_present()
        with pytest.raises(TimeoutError):
            await uds.tester_present()
        limited = UDSClient(ecu.transport, UDSConfig(p2_timeout=0.2, request_timeout=0.05))
        with pytest.raises(TimeoutError), limited.suppress_positive_responses():
            await limited.tester_present()


async def test_cancelled_exchange_drains_before_next_request(ecu_factory: EcuFactory) -> None:
    async with ecu_factory(case="pending-positive") as ecu:
        uds = UDSClient(ecu.transport, UDSConfig(p2_timeout=0.1, p2_star_timeout=0.2))
        async with ecu.adapter.recv() as frames:
            with uds.suppress_positive_responses():
                active = asyncio.create_task(uds.tester_present())
            async with asyncio.timeout(2):
                while True:
                    frame = await anext(frames)
                    if frame.id == Identifier.standard(0x7A9) and frame.data[1:4] == b"\x7f\x3e\x78":
                        break
            active.cancel()
            with pytest.raises(asyncio.CancelledError):
                await active
            with uds.suppress_positive_responses():
                await uds.tester_present()
            final_seen = False
            async with asyncio.timeout(2):
                while True:
                    frame = await anext(frames)
                    if frame.id == Identifier.standard(0x7A9) and frame.data[1:3] == b"\x7e\x00":
                        final_seen = True
                    if frame.id == Identifier.standard(0x7A1) and frame.data[1:3] == b"\x3e\x80":
                        assert final_seen, "second request overtook the cancelled exchange's final response"
                        break


async def test_closing_adapter_interrupts_pending_exchange(ecu_factory: EcuFactory) -> None:
    async with ecu_factory(case="silent") as ecu:
        uds = UDSClient(ecu.transport, UDSConfig(p2_timeout=5.0))
        async with ecu.adapter.recv() as frames:
            pending = asyncio.create_task(uds.tester_present())
            async with asyncio.timeout(2):
                while (await anext(frames)).id != Identifier.standard(0x7A1):
                    pass
            await ecu.adapter.aclose()
            with pytest.raises(ConnectionError):
                await asyncio.wait_for(pending, 1)


async def test_client_retains_adapter_after_python_owner_is_released(ecu_factory: EcuFactory) -> None:
    async with ecu_factory():
        adapter = SocketCanAdapter("vcan0")
        async with IsoTP.from_id(adapter, 0x7A1) as transport:
            client = UDSClient(transport, UDSConfig(p2_timeout=1.0))
            del adapter
            await client.tester_present()
            assert await client.read_data_by_identifier(0x1234) == b"deadbeef"


async def test_suppression_scopes_are_nested_and_task_local(ecu_factory: EcuFactory) -> None:
    async with ecu_factory() as ecu, ecu.adapter.recv() as frames:
        uds = UDSClient(ecu.transport, UDSConfig(p2_timeout=0.1))
        ready = asyncio.Event()

        async def existing_task() -> None:
            await ready.wait()
            await uds.tester_present()

        plain = asyncio.create_task(existing_task())
        with uds.suppress_positive_responses():
            ready.set()
            await plain
            await uds.tester_present()
            with pytest.raises(LookupError), uds.suppress_positive_responses():
                await uds.tester_present()
                raise LookupError("exit nested scope with an exception")
            await uds.tester_present()
        await uds.tester_present()

        sub_functions = []
        async with asyncio.timeout(2):
            async for frame in frames:
                if frame.id == Identifier.standard(0x7A1) and frame.data[1:2] == b"\x3e":
                    sub_functions.append(frame.data[2])
                if len(sub_functions) == 5:
                    break
        assert sub_functions == [0, 0x80, 0x80, 0x80, 0]
