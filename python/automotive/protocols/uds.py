"""Unified Diagnostic Services with Rust timing, parsing, and validation.

Methods accept named IntEnum members or raw integers. All byte inputs accept
bytes, bytearray, and contiguous byte-oriented memoryviews; results use bytes.
Each transport serializes exchanges, while separate ECU transports can run
concurrently. Negative responses raise NegativeResponseError with a raw code;
deadline expiry raises TimeoutError. See automotive.errors for other failures.
"""

from __future__ import annotations

from collections.abc import Generator
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass
from typing import Any, Self

from .. import _native
from .._base import Buffer, buffer_bytes, optional_bytes
from .._enums import (
    DataIdentifier,
    DTCFormatIdentifier,
    NegativeResponseCode,
    ReportType,
    ResetType,
    RoutineControlType,
    RoutineIdentifier,
    SecurityAccessType,
    ServiceIdentifier,
    SessionType,
)
from ..transports import Transport

_DEFAULTS = _native.defaults()


@dataclass(frozen=True, slots=True, kw_only=True)
class UDSConfig:
    """Reusable UDS timing and validation settings; durations are seconds.

    Args:
        p2_timeout: Initial response window, default 0.05 seconds.
        p2_star_timeout: Response window after each NRC 0x78, default 5 seconds.
        request_timeout: Overall exchange limit including sending and repeated
            pending responses. None (default) imposes no overall deadline.
        use_server_timing: Adopt timings returned by session control, default True.
        strict: Enable Rust's strict response validation, default True. Minimum
            parsing lengths still apply when False; unknown codes are preserved.

    Transport timeouts apply independently and must accommodate UDS waits.
    Configured/server timing includes complete response reception. Durations
    must be finite and nonnegative; values are otherwise passed directly to Rust.
    """

    p2_timeout: float = _DEFAULTS["p2_timeout"]
    p2_star_timeout: float = _DEFAULTS["p2_star_timeout"]
    request_timeout: float | None = _DEFAULTS["request_timeout"]
    use_server_timing: bool = _DEFAULTS["use_server_timing"]
    strict: bool = _DEFAULTS["strict"]

    def __post_init__(self) -> None:
        _native.validate_uds(self)


@dataclass(frozen=True, slots=True)
class SessionParameterRecord:
    """Server timings returned by DiagnosticSessionControl, in seconds.

    Attributes:
        p2_server_max: Initial response timing reported by the ECU.
        p2_star_server_max: Response-pending timing reported by the ECU.
    """

    p2_server_max: float
    p2_star_server_max: float


@dataclass(frozen=True, slots=True)
class DTCReportNumberByStatusMask:
    """DTC count and metadata returned by ReadDTCInformation.

    Attributes:
        dtc_status_availability_mask: ECU-supported status bits.
        dtc_format_identifier: Named DTCFormatIdentifier, or an unknown raw integer.
        dtc_count: Number of DTCs matching the requested status mask.
    """

    dtc_status_availability_mask: int
    dtc_format_identifier: DTCFormatIdentifier | int
    dtc_count: int


class UDSClient:
    """A persistent diagnostic client retaining its transport and session timing.

    Args:
        transport: Open message transport, such as software or native J2534 ISO-TP.
        config: Reusable timing/validation settings. None uses Rust defaults.

    Calls on the same transport are serialized. Suppression scopes retain this
    client's learned timing. Cancelling queued work prevents transmission;
    cancelling an active call ends the Python await but lets Rust drain the
    exchange under the transaction lock. Closing the transport interrupts it.
    Cancellation cannot undo ECU actions. Requests with unlimited NRC 0x78
    responses may hold the lock indefinitely unless request_timeout is set.

    Raises:
        NegativeResponseError: An ECU rejects the request; code is its raw NRC.
        ProtocolDeviationError: A strict validation rule fails; response is the
            full received PDU and deviation describes the mismatch.
        TimeoutError: A transport, response, or overall deadline expires.
        ValueError: Rust rejects a service argument before transmission.
        ConnectionError: The underlying connection closes during an exchange.

    Examples:
        With an open transport and ECU::

            uds = UDSClient(transport, UDSConfig(request_timeout=5.0))
            vin = await uds.read_data_by_identifier(DataIdentifier.VIN)
    """

    def __init__(self, transport: Transport, config: UDSConfig | None = None) -> None:
        self.transport = transport
        self.config = config if config is not None else UDSConfig()
        self._native = transport._native.uds(self.config)
        self._suppressed: ContextVar[bool] = ContextVar("automotive_uds_suppressed", default=False)

    async def _call(self, method: str, *args: object) -> Any:
        return await self._native.call(method, args, self._suppressed.get())

    @contextmanager
    def suppress_positive_responses(self) -> Generator[Self, None, None]:
        """Suppress positive responses within an ordinary with block.

        The scope is local to the current async task, supports nesting, and
        restores its previous setting even after exceptions. Tasks created
        inside the scope inherit it through Python's normal context propagation;
        already-running tasks are unchanged. No connection is opened or closed.

        Negative responses still raise; NRC 0x78 requires a final response.
        Services without sub-functions continue to require positive responses.

        Yields:
            This client, optionally bindable with ``as uds``.

        Examples:
            With an open client and ECU::

                with uds.suppress_positive_responses():
                    await uds.tester_present()
                await uds.tester_present()  # Requires a positive response again.
        """
        token = self._suppressed.set(True)
        try:
            yield self
        finally:
            self._suppressed.reset(token)

    async def request(
        self, sid: ServiceIdentifier | int, sub_function: int | None = None, data: Buffer | None = None
    ) -> bytes | None:
        """Send a custom UDS request and return positive response data.

        Args:
            sid: ServiceIdentifier or raw unsigned byte.
            sub_function: Optional raw byte. Bit 7 requests suppression.
            data: Optional service-specific bytes following the headers.

        Returns:
            Response data excluding SID and echoed sub-function, including
            b"" for an empty response. None means successful suppressed silence;
            it does not confirm ECU execution.

        Rust handles NRC 0x78 and response validation. A pending response makes
        subsequent silence a timeout rather than successful suppression.
        """
        return await self._call("request", sid, sub_function, optional_bytes(data))

    async def diagnostic_session_control(self, session_type: SessionType | int) -> SessionParameterRecord | None:
        """Select a diagnostic session (SID 0x10).

        Args:
            session_type: Named session or raw byte, including suppression flags.

        Returns:
            Server timing in seconds, or None for suppression/non-strict responses
            without a timing record. Rust adopts the record when configured.
        """
        value = await self._call("diagnostic_session_control", session_type)
        return None if value is None else SessionParameterRecord(**value)

    async def ecu_reset(self, reset_type: ResetType | int) -> int | None:
        """Request an ECU reset (SID 0x11).

        Args:
            reset_type: Named reset or raw byte, including suppression flags.

        Returns:
            Optional power-down time as a raw wire integer; None when absent
            or when the positive response is suppressed.
        """
        return await self._call("ecu_reset", reset_type)

    async def security_access(self, access_type: SecurityAccessType | int, data: Buffer | None = None) -> bytes | None:
        """Request a security seed or submit a key (SID 0x27).

        Args:
            access_type: Named access type or raw byte.
            data: Optional key or manufacturer-specific request bytes.

        Returns:
            Seed bytes, or None for SendKey acknowledgement/suppressed silence.

        Seed/key calculation remains the caller's responsibility.
        """
        return await self._call("security_access", access_type, optional_bytes(data))

    async def tester_present(self) -> None:
        """Send TesterPresent (SID 0x3E), awaiting the configured response window.

        A suppression scope accepts initial silence but still handles negative
        responses and requires a final response after NRC 0x78.
        """
        await self._call("tester_present")

    async def read_data_by_identifier(self, data_identifier: DataIdentifier | int) -> bytes:
        """Read a DID (SID 0x22) and return its data record as bytes.

        Args:
            data_identifier: Named DID or raw unsigned 16-bit integer.

        The echoed DID is checked in strict mode. No text decoding is applied.
        """
        return await self._call("read_data_by_identifier", data_identifier)

    async def write_data_by_identifier(self, data_identifier: DataIdentifier | int, data_record: Buffer) -> None:
        """Write a DID (SID 0x2E) and validate its acknowledgement.

        Args:
            data_identifier: Named DID or raw unsigned 16-bit integer.
            data_record: New data record; copied before asynchronous work.
        """
        await self._call("write_data_by_identifier", data_identifier, buffer_bytes(data_record))

    async def read_memory_by_address(self, memory_address: Buffer, memory_size: Buffer) -> bytes:
        """Read memory (SID 0x23) using explicit big-endian byte fields.

        Args:
            memory_address: Big-endian address bytes; leading zeros are retained.
            memory_size: Big-endian size bytes; its length determines wire width.

        Returns:
            Memory data bytes. Use the _alfid variant for integer arguments.
        """
        return await self._call("read_memory_by_address", buffer_bytes(memory_address), buffer_bytes(memory_size))

    async def read_memory_by_address_alfid(
        self, memory_address: int, memory_size: int, address_width: int, size_width: int
    ) -> bytes:
        """Read memory using integer address/size with explicit byte widths.

        Args:
            memory_address: Unsigned 64-bit memory address.
            memory_size: Unsigned 64-bit byte count.
            address_width: Address byte count, 1–8.
            size_width: Size byte count, 1–8.

        Raises:
            ValueError: A width is invalid or the value does not fit it.
        """
        return await self._call("read_memory_by_address_alfid", memory_address, memory_size, address_width, size_width)

    async def write_memory_by_address(self, memory_address: Buffer, memory_size: Buffer, data: Buffer) -> None:
        """Write memory (SID 0x3D) using big-endian address/size byte fields.

        Args:
            memory_address: Big-endian address, retaining leading zeros.
            memory_size: Big-endian byte count, retaining its explicit width.
            data: Memory bytes to write; must match the requested size.
        """
        await self._call(
            "write_memory_by_address", buffer_bytes(memory_address), buffer_bytes(memory_size), buffer_bytes(data)
        )

    async def write_memory_by_address_alfid(
        self, memory_address: int, memory_size: int, address_width: int, size_width: int, data: Buffer
    ) -> None:
        """Write memory with unsigned 64-bit address/size and byte widths 1–8.

        Args:
            memory_address: Numeric memory address.
            memory_size: Number of memory bytes to write.
            address_width: Address field width in bytes.
            size_width: Size field width in bytes.
            data: Memory data, copied before asynchronous work.

        Rust rejects invalid widths or values that do not fit before sending.
        """
        await self._call(
            "write_memory_by_address_alfid", memory_address, memory_size, address_width, size_width, buffer_bytes(data)
        )

    async def read_dtc_information_number_of_dtc_by_status_mask(self, mask: int) -> DTCReportNumberByStatusMask | None:
        """Return the number of DTCs matching a status mask (SID 0x19/0x01).

        Args:
            mask: Unsigned byte containing the requested status bits.

        Returns:
            DTC count/metadata, or None for suppressed silence. Unknown format
            identifiers are returned as raw integers instead of being rejected.
        """
        value = await self._call("read_dtc_information_number_of_dtc_by_status_mask", mask)
        if value is None:
            return None
        code = value["dtc_format_identifier"]
        try:
            value["dtc_format_identifier"] = DTCFormatIdentifier(code)
        except ValueError:
            pass
        return DTCReportNumberByStatusMask(**value)

    async def routine_control(
        self,
        routine_control_type: RoutineControlType | int,
        routine_identifier: RoutineIdentifier | int,
        data: Buffer | None = None,
    ) -> bytes | None:
        """Start, stop, or query a routine (SID 0x31).

        Args:
            routine_control_type: Named operation or raw byte.
            routine_identifier: Named routine or raw unsigned 16-bit ID.
            data: Optional routine-specific request record.

        Returns:
            Optional result record; None when absent or suppressed. Rust handles
            pending responses and validates the echoed routine in strict mode.
        """
        return await self._call("routine_control", routine_control_type, routine_identifier, optional_bytes(data))

    async def request_download(
        self, compression_method: int, encryption_method: int, memory_address: Buffer, memory_size: Buffer
    ) -> int:
        """Begin a client-to-ECU transfer (SID 0x34).

        Args:
            compression_method: Unshifted compression nibble, 0–15; zero is none.
            encryption_method: Unshifted encryption nibble, 0–15; zero is none.
            memory_address: Big-endian address bytes.
            memory_size: Big-endian total byte count.

        Returns:
            Maximum complete TransferData request size, including SID/counter.
        """
        return await self._call(
            "request_download",
            compression_method,
            encryption_method,
            buffer_bytes(memory_address),
            buffer_bytes(memory_size),
        )

    async def request_download_alfid(
        self,
        compression_method: int,
        encryption_method: int,
        memory_address: int,
        memory_size: int,
        address_width: int,
        size_width: int,
    ) -> int:
        """Begin a download using integer address/size and explicit byte widths.

        Args:
            compression_method: Unshifted nibble, 0–15.
            encryption_method: Unshifted nibble, 0–15.
            memory_address: Unsigned 64-bit address.
            memory_size: Unsigned 64-bit total byte count.
            address_width: Address field length, 1–8 bytes.
            size_width: Size field length, 1–8 bytes.

        Returns:
            Maximum complete TransferData request size, including SID/counter.
        """
        return await self._call(
            "request_download_alfid",
            compression_method,
            encryption_method,
            memory_address,
            memory_size,
            address_width,
            size_width,
        )

    async def request_upload(
        self, compression_method: int, encryption_method: int, memory_address: Buffer, memory_size: Buffer
    ) -> int:
        """Begin an ECU-to-client transfer (SID 0x35).

        Args:
            compression_method: Unshifted nibble, 0–15; zero is none.
            encryption_method: Unshifted nibble, 0–15; zero is none.
            memory_address: Big-endian address bytes.
            memory_size: Big-endian total byte count.

        Returns:
            Maximum complete TransferData response size, including SID/counter.
        """
        return await self._call(
            "request_upload",
            compression_method,
            encryption_method,
            buffer_bytes(memory_address),
            buffer_bytes(memory_size),
        )

    async def request_upload_alfid(
        self,
        compression_method: int,
        encryption_method: int,
        memory_address: int,
        memory_size: int,
        address_width: int,
        size_width: int,
    ) -> int:
        """Begin an upload using integer fields with explicit byte widths.

        Args:
            compression_method: Unshifted nibble, 0–15.
            encryption_method: Unshifted nibble, 0–15.
            memory_address: Unsigned 64-bit address.
            memory_size: Unsigned 64-bit total byte count.
            address_width: Address byte count, 1–8.
            size_width: Size byte count, 1–8.

        Returns:
            Maximum complete TransferData response size, including SID/counter.
        """
        return await self._call(
            "request_upload_alfid",
            compression_method,
            encryption_method,
            memory_address,
            memory_size,
            address_width,
            size_width,
        )

    async def transfer_data(self, block_sequence_counter: int, data: Buffer | None = None) -> bytes | None:
        """Transfer one block (SID 0x36), validating the echoed counter.

        Args:
            block_sequence_counter: Unsigned wire byte; first block is usually 1.
            data: Download data. None requests an upload block.

        Returns:
            Upload bytes or optional download response data; None when absent.
        """
        return await self._call("transfer_data", block_sequence_counter, optional_bytes(data))

    async def request_transfer_exit(self, data: Buffer | None = None) -> bytes | None:
        """Finish an upload/download (SID 0x37).

        Args:
            data: Optional service-specific completion record, such as a checksum.

        Returns:
            Optional ECU completion data, or None when no record is returned.
        """
        return await self._call("request_transfer_exit", optional_bytes(data))


__all__ = [
    "DTCFormatIdentifier",
    "DTCReportNumberByStatusMask",
    "DataIdentifier",
    "NegativeResponseCode",
    "ReportType",
    "ResetType",
    "RoutineControlType",
    "RoutineIdentifier",
    "SecurityAccessType",
    "ServiceIdentifier",
    "SessionParameterRecord",
    "SessionType",
    "UDSClient",
    "UDSConfig",
]
