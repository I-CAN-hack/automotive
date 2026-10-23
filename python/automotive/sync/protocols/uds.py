"""Blocking UDS methods forwarding to the existing async client."""

from __future__ import annotations

from collections.abc import Generator
from contextlib import contextmanager
from typing import Self

from ..._base import Buffer
from ...protocols import uds as _uds
from ...protocols.uds import (
    DataIdentifier,
    DTCFormatIdentifier,
    DTCReportNumberByStatusMask,
    NegativeResponseCode,
    ReportType,
    ResetType,
    RoutineControlType,
    RoutineIdentifier,
    SecurityAccessType,
    ServiceIdentifier,
    SessionParameterRecord,
    SessionType,
    UDSConfig,
)
from .._runtime import call, run
from ..transports.isotp import IsoTPAdapter


class UDSClient:
    """A blocking diagnostic client retaining its transport and session timing.

    Args:
        transport: Open synchronous software or native J2534 ISO-TP transport.
        config: Shared timing and validation settings; None uses Rust defaults.

    Calls on one transport are serialized by Rust. Results and exceptions match
    the async client. Ctrl+C interrupts the Python wait; an active Rust exchange
    drains under the transaction lock before another request can start. Closing
    the parent interrupts dependent operations. Set request_timeout to bound
    repeated response-pending messages.

    Raises:
        NegativeResponseError: The ECU rejected the request; code is its raw NRC.
        ProtocolDeviationError: Strict validation failed; response retains the PDU.
        TimeoutError: A transport, response, or overall deadline expired.
        ValueError: Rust rejected an argument before transmission.
        ConnectionError: The connection closed during an exchange.
    """

    def __init__(self, transport: IsoTPAdapter, config: UDSConfig | None = None) -> None:
        self.transport = transport
        self._async = call(_uds.UDSClient, transport._async, config)
        self.config = self._async.config

    @contextmanager
    def suppress_positive_responses(self) -> Generator[Self, None, None]:
        """Suppress positive responses within an ordinary with block.

        Nested scopes restore their previous setting, including after exceptions.
        The scope is local to the calling thread and context. Negative responses
        still raise; NRC 0x78 requires a final response. Services without a
        sub-function continue to require positive responses.

        Yields:
            This synchronous client, optionally bindable with ``as uds``.
        """
        with self._async.suppress_positive_responses():
            yield self

    def request(
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
        return run(self._async.request, sid, sub_function, data)

    def diagnostic_session_control(self, session_type: SessionType | int) -> SessionParameterRecord | None:
        """Select a diagnostic session (SID 0x10).

        Args:
            session_type: Named session or raw byte, including suppression flags.

        Returns:
            Server timing in seconds, or None for suppression/non-strict responses
            without a timing record. Rust adopts the record when configured.
        """
        return run(self._async.diagnostic_session_control, session_type)

    def ecu_reset(self, reset_type: ResetType | int) -> int | None:
        """Request an ECU reset (SID 0x11).

        Args:
            reset_type: Named reset or raw byte, including suppression flags.

        Returns:
            Optional power-down time as a raw wire integer; None when absent
            or when the positive response is suppressed.
        """
        return run(self._async.ecu_reset, reset_type)

    def security_access(self, access_type: SecurityAccessType | int, data: Buffer | None = None) -> bytes | None:
        """Request a security seed or submit a key (SID 0x27).

        Args:
            access_type: Named access type or raw byte.
            data: Optional key or manufacturer-specific request bytes.

        Returns:
            Seed bytes, or None for SendKey acknowledgement/suppressed silence.

        Seed/key calculation remains the caller's responsibility.
        """
        return run(self._async.security_access, access_type, data)

    def tester_present(self) -> None:
        """Send TesterPresent (SID 0x3E), blocking for the configured response window.

        A suppression scope accepts initial silence but still handles negative
        responses and requires a final response after NRC 0x78.
        """
        return run(self._async.tester_present)

    def read_data_by_identifier(self, data_identifier: DataIdentifier | int) -> bytes:
        """Read a DID (SID 0x22) and return its data record as bytes.

        Args:
            data_identifier: Named DID or raw unsigned 16-bit integer.

        The echoed DID is checked in strict mode. No text decoding is applied.
        """
        return run(self._async.read_data_by_identifier, data_identifier)

    def write_data_by_identifier(self, data_identifier: DataIdentifier | int, data_record: Buffer) -> None:
        """Write a DID (SID 0x2E) and validate its acknowledgement.

        Args:
            data_identifier: Named DID or raw unsigned 16-bit integer.
            data_record: New data record; copied before transmission.
        """
        return run(self._async.write_data_by_identifier, data_identifier, data_record)

    def read_memory_by_address(self, memory_address: Buffer, memory_size: Buffer) -> bytes:
        """Read memory (SID 0x23) using explicit big-endian byte fields.

        Args:
            memory_address: Big-endian address bytes; leading zeros are retained.
            memory_size: Big-endian size bytes; its length determines wire width.

        Returns:
            Memory data bytes. Use the _alfid variant for integer arguments.
        """
        return run(self._async.read_memory_by_address, memory_address, memory_size)

    def read_memory_by_address_alfid(
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
        return run(self._async.read_memory_by_address_alfid, memory_address, memory_size, address_width, size_width)

    def write_memory_by_address(self, memory_address: Buffer, memory_size: Buffer, data: Buffer) -> None:
        """Write memory (SID 0x3D) using big-endian address/size byte fields.

        Args:
            memory_address: Big-endian address, retaining leading zeros.
            memory_size: Big-endian byte count, retaining its explicit width.
            data: Memory bytes to write; must match the requested size.
        """
        return run(self._async.write_memory_by_address, memory_address, memory_size, data)

    def write_memory_by_address_alfid(
        self, memory_address: int, memory_size: int, address_width: int, size_width: int, data: Buffer
    ) -> None:
        """Write memory with unsigned 64-bit address/size and byte widths 1–8.

        Args:
            memory_address: Numeric memory address.
            memory_size: Number of memory bytes to write.
            address_width: Address field width in bytes.
            size_width: Size field width in bytes.
            data: Memory data, copied before transmission.

        Rust rejects invalid widths or values that do not fit before sending.
        """
        return run(
            self._async.write_memory_by_address_alfid, memory_address, memory_size, address_width, size_width, data
        )

    def read_dtc_information_number_of_dtc_by_status_mask(self, mask: int) -> DTCReportNumberByStatusMask | None:
        """Return the number of DTCs matching a status mask (SID 0x19/0x01).

        Args:
            mask: Unsigned byte containing the requested status bits.

        Returns:
            DTC count/metadata, or None for suppressed silence. Unknown format
            identifiers are returned as raw integers instead of being rejected.
        """
        return run(self._async.read_dtc_information_number_of_dtc_by_status_mask, mask)

    def routine_control(
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
        return run(self._async.routine_control, routine_control_type, routine_identifier, data)

    def request_download(
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
        return run(self._async.request_download, compression_method, encryption_method, memory_address, memory_size)

    def request_download_alfid(
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
        return run(
            self._async.request_download_alfid,
            compression_method,
            encryption_method,
            memory_address,
            memory_size,
            address_width,
            size_width,
        )

    def request_upload(
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
        return run(self._async.request_upload, compression_method, encryption_method, memory_address, memory_size)

    def request_upload_alfid(
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
        return run(
            self._async.request_upload_alfid,
            compression_method,
            encryption_method,
            memory_address,
            memory_size,
            address_width,
            size_width,
        )

    def transfer_data(self, block_sequence_counter: int, data: Buffer | None = None) -> bytes | None:
        """Transfer one block (SID 0x36), validating the echoed counter.

        Args:
            block_sequence_counter: Unsigned wire byte; first block is usually 1.
            data: Download data. None requests an upload block.

        Returns:
            Upload bytes or optional download response data; None when absent.
        """
        return run(self._async.transfer_data, block_sequence_counter, data)

    def request_transfer_exit(self, data: Buffer | None = None) -> bytes | None:
        """Finish an upload/download (SID 0x37).

        Args:
            data: Optional service-specific completion record, such as a checksum.

        Returns:
            Optional ECU completion data, or None when no record is returned.
        """
        return run(self._async.request_transfer_exit, data)


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
