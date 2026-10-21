//! Unified Diagnostic Services (UDS) Client, implements ISO 14229
//!
//! [`UDSClient::new`] uses [`UDSConfig::default`], including strict validation.
//! Customize P2/P2*, overall request timeout, and adoption of session timings
//! with [`UDSConfig::builder`] and [`UDSClient::with_config`].
//! Strict validation failures include the complete response in
//! [`Error::ProtocolDeviation`]. Minimum safe parsing lengths always apply.
//! Strict response validation follows ISO 14229-1:2013: negative-response
//! structure and request SID (§7.4), sub-function echoes (§8.3.2), service-specific
//! records (§9–§14), and NRC/DTC format values (Annex A.1 and Annex D.4).
//! Memory reads return the requested byte count (§10.3), and memory-write
//! responses echo the ALFID, address, and size (§10.8.3).
//!
//! ## Example
//! ```rust
//! async fn uds_example() -> automotive::Result<()> {
//!     let adapter = automotive::adapters::get_adapter()?;
//!     let isotp = automotive::transports::isotp::IsoTPAdapter::from_id(&adapter, 0x7a1);
//!     let uds = automotive::protocols::uds::UDSClient::new(&isotp);
//!
//!     uds.tester_present().await?;
//!     let response = uds.read_data_by_identifier(automotive::protocols::uds::DataIdentifier::ApplicationSoftwareIdentification as u16).await?;
//!
//!     println!("Application Software Identification: {}", hex::encode(response));
//!     Ok(())
//! }

mod config;
mod constants;
mod error;
mod types;

use crate::transports::TransportLayer;
use crate::Result;
use crate::StreamExt;
pub use config::{UDSConfig, UDSConfigBuilder};
pub use constants::*;
pub use error::{Error, NegativeResponseCode, ProtocolDeviation};
pub use types::*;

use std::sync::RwLock;
use tracing::info;

struct Response {
    payload: Vec<u8>,
    data_start: usize,
}

impl Response {
    fn data(&self) -> &[u8] {
        &self.payload[self.data_start..]
    }

    fn deviation(self, deviation: ProtocolDeviation) -> crate::Error {
        Error::ProtocolDeviation {
            deviation,
            response: self.payload,
        }
        .into()
    }
}

/// UDS Client. Generic over any [`TransportLayer`] implementation so that it
/// works equally with software ISO-TP ([`crate::transports::isotp::IsoTPAdapter`]) and
/// hardware ISO 15765 (e.g. `J2534NativeIsoTpTransport`).
pub struct UDSClient<'a, T: TransportLayer> {
    adapter: &'a T,
    config: UDSConfig,
    session_timing: RwLock<SessionParameterRecord>,
}

impl<'a, T: TransportLayer> UDSClient<'a, T> {
    pub fn new(adapter: &'a T) -> Self {
        Self::with_config(adapter, UDSConfig::default())
    }

    /// Create a client with custom response timing and validation settings.
    pub fn with_config(adapter: &'a T, config: UDSConfig) -> Self {
        Self {
            adapter,
            config,
            session_timing: RwLock::new(SessionParameterRecord {
                p2_server_max: config.p2_timeout,
                p2_star_server_max: config.p2_star_timeout,
            }),
        }
    }

    /// Make a custom UDS request, handle negative responses, and return response
    /// data. Strict mode verifies response headers and negative-response PDUs.
    /// Service methods also validate service-specific records. Deviations include
    /// the full received PDU in [`Error::ProtocolDeviation`].
    pub async fn request(
        &self,
        sid: u8,
        sub_function: Option<u8>,
        data: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        Ok(self
            .request_response(sid, sub_function, data)
            .await?
            .data()
            .to_vec())
    }

    async fn request_response(
        &self,
        sid: u8,
        sub_function: Option<u8>,
        data: Option<&[u8]>,
    ) -> Result<Response> {
        let exchange = self.request_inner(sid, sub_function, data);
        match self.config.request_timeout {
            Some(timeout) => tokio::time::timeout(timeout, exchange)
                .await
                .map_err(|_| crate::Error::Timeout)?,
            None => exchange.await,
        }
    }

    async fn request_inner(
        &self,
        sid: u8,
        sub_function: Option<u8>,
        data: Option<&[u8]>,
    ) -> Result<Response> {
        let mut request: Vec<u8> = vec![sid];

        if let Some(sub_function) = sub_function {
            request.push(sub_function);
        }

        if let Some(data) = data {
            request.extend(data);
        }

        let mut stream = self.adapter.recv();

        self.adapter.send(&request).await?;

        let timing = *self.session_timing.read().unwrap();
        let mut response_timeout = timing.p2_server_max;
        loop {
            let response = tokio::time::timeout(response_timeout, stream.next())
                .await
                .map_err(|_| crate::Error::Timeout)?
                .ok_or(crate::Error::Disconnected)??;

            // Ensure we got a reply at all
            if response.is_empty() {
                return Err(self.invalid_response_length(response, 1, None));
            }

            // Check for errors
            let response_sid = response[0];
            if response_sid == NEGATIVE_RESPONSE {
                // ISO 14229-1:2013, §7.4: [0x7F, request SID, NRC].
                if response.len() < 3 || (self.config.strict && response.len() != 3) {
                    return Err(self.invalid_response_length(response, 3, Some(3)));
                }
                if self.config.strict && response[1] != sid {
                    let actual = response[1];
                    return Err(Response {
                        payload: response,
                        data_start: 0,
                    }
                    .deviation(ProtocolDeviation::InvalidRequestServiceId {
                        expected: sid,
                        actual,
                    }));
                }
                // Annex A.1 reserves 0x38–0x4F for extended data-link security
                // and 0xF0–0xFE for vehicle-manufacturer conditions.
                if self.config.strict
                    && !matches!(response[2],
                    0x10..=0x14 | 0x21..=0x22 | 0x24..=0x26 | 0x31 | 0x33 |
                    0x35..=0x4f | 0x70..=0x73 | 0x78 | 0x7e..=0x7f |
                    0x81..=0x8d | 0x8f..=0x93 | 0xf0..=0xfe)
                {
                    let code = response[2];
                    return Err(Response {
                        payload: response,
                        data_start: 0,
                    }
                    .deviation(ProtocolDeviation::InvalidNegativeResponseCode(code)));
                }

                let code: NegativeResponseCode = response[2].into();
                if code == NegativeResponseCode::RequestCorrectlyReceivedResponsePending {
                    info!("Received Response Pending");
                    response_timeout = timing.p2_star_server_max;
                    continue;
                }

                return Err(Error::NegativeResponse(code).into());
            }

            // Check service id
            if self.config.strict && response_sid != sid | POSITIVE_RESPONSE {
                return Err(Response {
                    payload: response,
                    data_start: 0,
                }
                .deviation(ProtocolDeviation::InvalidServiceId {
                    expected: sid | POSITIVE_RESPONSE,
                    actual: response_sid,
                }));
            }

            // Check sub function
            if let Some(sub_function) = sub_function {
                // Ensure we also have a subfunction in the response
                if response.len() <= 1 {
                    return Err(self.invalid_response_length(response, 2, None));
                }

                // The suppression bit is not echoed (§8.3.2, Table 17).
                let expected = sub_function & 0x7f;
                if self.config.strict && response[1] != expected {
                    let actual = response[1];
                    return Err(Response {
                        payload: response,
                        data_start: 0,
                    }
                    .deviation(ProtocolDeviation::InvalidSubFunction { expected, actual }));
                }
            }

            let start: usize = if sub_function.is_some() { 2 } else { 1 };
            return Ok(Response {
                payload: response,
                data_start: start,
            });
        }
    }

    fn invalid_response_length(
        &self,
        response: Vec<u8>,
        minimum: usize,
        maximum: Option<usize>,
    ) -> crate::Error {
        if self.config.strict {
            Error::ProtocolDeviation {
                deviation: ProtocolDeviation::InvalidResponseLength { minimum, maximum },
                response,
            }
            .into()
        } else {
            Error::InvalidResponseLength.into()
        }
    }

    fn validate_response_length(
        &self,
        response: Response,
        minimum: usize,
        maximum: Option<usize>,
    ) -> Result<Response> {
        let length = response.payload.len();
        if self.config.strict
            && (length < minimum || maximum.is_some_and(|maximum| length > maximum))
        {
            return Err(self.invalid_response_length(response.payload, minimum, maximum));
        }
        Ok(response)
    }

    /// 0x10 - Diagnostic Session Control. Strict mode requires four bytes of
    /// session timing information (ISO 14229-1:2013, §9.2.3). These timings apply
    /// to subsequent requests unless [`UDSConfig::use_server_timing`] is disabled.
    pub async fn diagnostic_session_control(
        &self,
        session_type: u8,
    ) -> Result<Option<types::SessionParameterRecord>> {
        let response = self
            .request_response(
                ServiceIdentifier::DiagnosticSessionControl as u8,
                Some(session_type),
                None,
            )
            .await?;
        let response = self.validate_response_length(response, 6, Some(6))?;
        let result = response.data();

        let result = if result.len() == 4 {
            let p2_server_max = u16::from_be_bytes([result[0], result[1]]);
            let p2_server_max = std::time::Duration::from_millis(p2_server_max as u64);
            let p2_star_server_max = u16::from_be_bytes([result[2], result[3]]);
            let p2_star_server_max =
                std::time::Duration::from_millis(p2_star_server_max as u64 * 10);

            Some(types::SessionParameterRecord {
                p2_server_max,
                p2_star_server_max,
            })
        } else {
            None
        };

        if self.config.use_server_timing {
            if let Some(timing) = result {
                *self.session_timing.write().unwrap() = timing;
            }
        }

        Ok(result)
    }

    /// 0x11 - ECU Reset. The `reset_type` parameter can be used to specify the type of reset to perform. Use the [`constants::ResetType`] enum for  the reset types defined in the standard. This function returns the power down time when the reset type is [`constants::ResetType::EnableRapidPowerShutDown`].
    pub async fn ecu_reset(&self, reset_type: u8) -> Result<Option<u8>> {
        let response = self
            .request_response(ServiceIdentifier::EcuReset as u8, Some(reset_type), None)
            .await?;
        // §9.3.3, Table 35: powerDownTime is present only for reset type 0x04.
        let expected = if reset_type & 0x7f == ResetType::EnableRapidPowerShutDown as u8 {
            3
        } else {
            2
        };
        let response = self.validate_response_length(response, expected, Some(expected))?;
        let result = response.data();

        let result = if result.len() == 1 {
            Some(result[0])
        } else {
            None
        };

        Ok(result)
    }

    /// 0x27 - Security Access. Odd `access_type` values are used to request a seed, even values to send a key. The `data` parameter is optional when requesting a seed. You can use the [`constants::SecurityAccessType`] enum for the default security level.
    pub async fn security_access(&self, access_type: u8, data: Option<&[u8]>) -> Result<Vec<u8>> {
        let send_key = access_type % 2 == 0;
        if send_key && data.is_none() {
            panic!("Missing data parameter when sending key");
        }

        let response = self
            .request_response(
                ServiceIdentifier::SecurityAccess as u8,
                Some(access_type),
                data,
            )
            .await?;
        // §9.4.3, Table 44: a seed is mandatory for requestSeed and absent for sendKey.
        let response = if send_key {
            self.validate_response_length(response, 2, Some(2))?
        } else {
            self.validate_response_length(response, 3, None)?
        };
        Ok(response.data().to_vec())
    }

    /// 0x3E - Tester Present
    pub async fn tester_present(&self) -> Result<()> {
        let response = self
            .request_response(ServiceIdentifier::TesterPresent as u8, Some(0), None)
            .await?;
        self.validate_response_length(response, 2, Some(2))?;
        Ok(())
    }

    async fn read_write_memory_by_adddress(
        &self,
        sid: ServiceIdentifier,
        memory_address: &[u8],
        memory_size: &[u8],
        data: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        assert!(
            sid == ServiceIdentifier::ReadMemoryByAddress
                || sid == ServiceIdentifier::WriteMemoryByAddress
        );
        assert!(!memory_address.is_empty() && memory_address.len() <= 0xF);
        assert!(!memory_size.is_empty() && memory_size.len() <= 0xF);

        let address_and_length_format =
            ((memory_size.len() as u8) << 4) | (memory_address.len() as u8);

        let mut buf: Vec<u8> = vec![address_and_length_format];
        buf.extend(memory_address);
        buf.extend(memory_size);
        if let Some(data) = data {
            buf.extend(data);
        }

        let response = self.request_response(sid as u8, None, Some(&buf)).await?;
        if self.config.strict {
            let resp = response.data();
            if sid == ServiceIdentifier::ReadMemoryByAddress {
                // §10.3.2–§10.3.3: memorySize is a big-endian byte count, up to 15 bytes.
                let expected = memory_size
                    .iter()
                    .fold(0u128, |size, &byte| (size << 8) | u128::from(byte));
                if expected != resp.len() as u128 {
                    let actual = resp.len();
                    return Err(
                        response.deviation(ProtocolDeviation::InvalidMemoryRecordLength {
                            expected,
                            actual,
                        }),
                    );
                }
                if resp.is_empty() {
                    return Err(self.invalid_response_length(response.payload, 2, None));
                }
            } else {
                // §10.8.3, Tables 240–241: ALFID, address, and size are echoed exactly.
                let expected_length = 2 + memory_address.len() + memory_size.len();
                let response = self.validate_response_length(
                    response,
                    expected_length,
                    Some(expected_length),
                )?;
                let resp = response.data();
                if resp[0] != address_and_length_format {
                    let actual = resp[0];
                    return Err(response.deviation(
                        ProtocolDeviation::InvalidAddressAndLengthFormatIdentifier {
                            expected: address_and_length_format,
                            actual,
                        },
                    ));
                }
                let size_start = 1 + memory_address.len();
                if &resp[1..size_start] != memory_address {
                    let actual = resp[1..size_start].to_vec();
                    return Err(response.deviation(ProtocolDeviation::InvalidMemoryAddress {
                        expected: memory_address.to_vec(),
                        actual,
                    }));
                }
                if &resp[size_start..] != memory_size {
                    let actual = resp[size_start..].to_vec();
                    return Err(response.deviation(ProtocolDeviation::InvalidMemorySize {
                        expected: memory_size.to_vec(),
                        actual,
                    }));
                }
                return Ok(resp.to_vec());
            }
        }
        Ok(response.data().to_vec())
    }

    /// 0x22 - Read Data By Identifier. Specify a 16 bit data identifier, or use a constant from [`constants::DataIdentifier`] for standardized identifiers. Reading multiple identifiers simultaneously is possible on some ECUs, but not supported by this function.
    pub async fn read_data_by_identifier(&self, data_identifier: u16) -> Result<Vec<u8>> {
        let did = data_identifier.to_be_bytes();
        let response = self
            .request_response(
                ServiceIdentifier::ReadDataByIdentifier as u8,
                None,
                Some(&did),
            )
            .await?;
        // §10.2.3, Table 144: at least one data-record byte is mandatory.
        let response = self.validate_response_length(response, 4, None)?;
        let resp = response.data();

        if resp.len() < 2 {
            return Err(self.invalid_response_length(response.payload, 3, None));
        }

        let did = u16::from_be_bytes([resp[0], resp[1]]);
        if self.config.strict && did != data_identifier {
            return Err(
                response.deviation(ProtocolDeviation::InvalidDataIdentifier {
                    expected: data_identifier,
                    actual: did,
                }),
            );
        }

        Ok(resp[2..].to_vec())
    }

    /// 0x23 - Read Memory By Address. The `memory_address` parameter should be the address to read from, and the `memory_size` parameter should be the number of bytes to read.
    pub async fn read_memory_by_address(
        &self,
        memory_address: &[u8],
        memory_size: &[u8],
    ) -> Result<Vec<u8>> {
        self.read_write_memory_by_adddress(
            ServiceIdentifier::ReadMemoryByAddress,
            memory_address,
            memory_size,
            None,
        )
        .await
    }

    /// 0x2E - Write Data By Identifier. Specify a 16 bit data identifier, or use a constant from [`constants::DataIdentifier`] for standardized identifiers.
    pub async fn write_data_by_identifier(
        &self,
        data_identifier: u16,
        data_record: &[u8],
    ) -> Result<()> {
        let mut data: Vec<u8> = data_identifier.to_be_bytes().to_vec();
        data.extend(data_record);

        let response = self
            .request_response(
                ServiceIdentifier::WriteDataByIdentifier as u8,
                None,
                Some(&data),
            )
            .await?;
        let response = self.validate_response_length(response, 3, Some(3))?;
        let resp = response.data();

        if resp.len() < 2 {
            return Err(self.invalid_response_length(response.payload, 3, None));
        }

        let did = u16::from_be_bytes([resp[0], resp[1]]);
        if self.config.strict && did != data_identifier {
            return Err(
                response.deviation(ProtocolDeviation::InvalidDataIdentifier {
                    expected: data_identifier,
                    actual: did,
                }),
            );
        }

        Ok(())
    }

    /// 0x3D - Write Memory By Address. The `memory_address` parameter should be the address to write to, and the `memory_size` parameter should be the number of bytes to write. The `data` parameter should be the data to write.
    pub async fn write_memory_by_address(
        &self,
        memory_address: &[u8],
        memory_size: &[u8],
        data: &[u8],
    ) -> Result<()> {
        self.read_write_memory_by_adddress(
            ServiceIdentifier::WriteMemoryByAddress,
            memory_address,
            memory_size,
            Some(data),
        )
        .await?;
        Ok(())
    }

    pub async fn read_dtc_information_number_of_dtc_by_status_mask(
        &self,
        mask: u8,
    ) -> Result<DTCReportNumberByStatusMask> {
        let response = self
            .request_response(
                ServiceIdentifier::ReadDTCInformation as u8,
                Some(ReportType::ReportNumberOfDTCByStatusMask as u8),
                Some(&[mask]),
            )
            .await?;
        let resp = response.data();

        if resp.len() < 4 || (self.config.strict && resp.len() != 4) {
            return Err(self.invalid_response_length(response.payload, 6, Some(6)));
        }

        let mask = resp[0];
        let Some(format) = DTCFormatIdentifier::from_repr(resp[1]) else {
            let format = resp[1];
            return Err(if self.config.strict {
                response.deviation(ProtocolDeviation::InvalidDTCFormatIdentifier(format))
            } else {
                Error::InvalidDTCFormatIdentifier(format).into()
            });
        };
        let count = u16::from_be_bytes([resp[2], resp[3]]);

        Ok(DTCReportNumberByStatusMask {
            dtc_status_availability_mask: mask,
            dtc_format_identifier: format,
            dtc_count: count,
        })
    }

    /// 0x31 - Routine Control. The `routine_control_type` selects the operation such as Start and Stop, see [`constants::RoutineControlType`]. The `routine_identifier` is a 16-bit identifier for the routine. The `data` parameter is optional and can be used when starting or stopping a routine. The ECU can optionally return data for all routine operations.
    pub async fn routine_control(
        &self,
        routine_control_type: constants::RoutineControlType,
        routine_identifier: u16,
        data: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>> {
        let mut buf: Vec<u8> = vec![];
        buf.extend(routine_identifier.to_be_bytes());
        if let Some(data) = data {
            buf.extend(data);
        }

        let response = self
            .request_response(
                ServiceIdentifier::RoutineControl as u8,
                Some(routine_control_type as u8),
                Some(&buf),
            )
            .await?;
        let resp = response.data();

        if resp.len() < 2 {
            return Err(self.invalid_response_length(response.payload, 4, None));
        }

        let id = u16::from_be_bytes([resp[0], resp[1]]);
        if self.config.strict && id != routine_identifier {
            return Err(
                response.deviation(ProtocolDeviation::InvalidRoutineIdentifier {
                    expected: routine_identifier,
                    actual: id,
                }),
            );
        }

        Ok(if resp.len() > 2 {
            Some(resp[2..].to_vec())
        } else {
            None
        })
    }

    async fn request_download_upload(
        &self,
        sid: ServiceIdentifier,
        compression_method: u8,
        encryption_method: u8,
        memory_address: &[u8],
        memory_size: &[u8],
    ) -> Result<usize> {
        assert!(
            sid == ServiceIdentifier::RequestDownload || sid == ServiceIdentifier::RequestUpload
        );
        assert!(compression_method <= 0xF);
        assert!(encryption_method <= 0xF);
        assert!(!memory_address.is_empty() && memory_address.len() <= 0xF);
        assert!(!memory_size.is_empty() && memory_size.len() <= 0xF);

        let data_format = (compression_method << 4) | encryption_method;
        let address_and_length_format =
            ((memory_size.len() as u8) << 4) | (memory_address.len() as u8);

        let mut data: Vec<u8> = vec![data_format, address_and_length_format];
        data.extend(memory_address);
        data.extend(memory_size);

        let response = self.request_response(sid as u8, None, Some(&data)).await?;
        let resp = response.data();

        // Ensure the response contains at least a length format
        if resp.is_empty() {
            return Err(self.invalid_response_length(response.payload, 2, None));
        }

        let num_length_bytes = (resp[0] >> 4) as usize;
        // §14.2.3 and §14.3.3: the low nibble is reserved and must be zero.
        if self.config.strict && resp[0] & 0x0f != 0 {
            let format = resp[0];
            return Err(
                response.deviation(ProtocolDeviation::InvalidLengthFormatIdentifier(format))
            );
        }
        if self.config.strict && num_length_bytes == 0 {
            return Err(
                response.deviation(ProtocolDeviation::InvalidLengthFormat(num_length_bytes))
            );
        }
        if resp.len() < num_length_bytes + 1
            || (self.config.strict && resp.len() != num_length_bytes + 1)
        {
            let expected = num_length_bytes + 2;
            return Err(self.invalid_response_length(response.payload, expected, Some(expected)));
        }

        // The four-bit length field permits up to 15 big-endian bytes.
        let length = resp[1..num_length_bytes + 1]
            .iter()
            .fold(0u128, |acc, &byte| (acc << 8) | u128::from(byte));

        // A complete TransferData message includes SID, counter, and data.
        if self.config.strict && length < 3 {
            return Err(
                response.deviation(ProtocolDeviation::InvalidMaxNumberOfBlockLength(length))
            );
        }

        usize::try_from(length).map_err(|_| {
            Error::BlockLengthOverflow {
                value: length,
                response: response.payload,
            }
            .into()
        })
    }

    /// 0x34 - Request Download. Initiates a transfer from the client to the ECU.
    /// Returns the maximum complete TransferData request length, including SID
    /// and block counter. Values above `usize::MAX` return [`Error::BlockLengthOverflow`].
    pub async fn request_download(
        &self,
        compression_method: u8,
        encryption_method: u8,
        memory_address: &[u8],
        memory_size: &[u8],
    ) -> Result<usize> {
        self.request_download_upload(
            ServiceIdentifier::RequestDownload,
            compression_method,
            encryption_method,
            memory_address,
            memory_size,
        )
        .await
    }

    /// 0x35 - Request Upload. Initiates a transfer from the ECU to the client.
    /// Returns the maximum complete TransferData response length, including SID
    /// and block counter. Values above `usize::MAX` return [`Error::BlockLengthOverflow`].
    pub async fn request_upload(
        &self,
        compression_method: u8,
        encryption_method: u8,
        memory_address: &[u8],
        memory_size: &[u8],
    ) -> Result<usize> {
        self.request_download_upload(
            ServiceIdentifier::RequestUpload,
            compression_method,
            encryption_method,
            memory_address,
            memory_size,
        )
        .await
    }

    /// 0x36 - Transfer Data. Used to transfer data to or from the ECU. The `data` parameter should be a slice of the data to transfer. The `transfer_request` parameter should be the sequence number of the transfer request, starting at 1. The `data` parameter should be `None` when an upload is requested, and the function will return the data received from the ECU. The `data` parameter should be `Some` when a download is requested, and the function will return `None`.
    pub async fn transfer_data(
        &self,
        block_sequence_counter: u8,
        data: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>> {
        let mut buf: Vec<u8> = vec![block_sequence_counter];
        if let Some(data) = data {
            buf.extend(data);
        }

        let response = self
            .request_response(ServiceIdentifier::TransferData as u8, None, Some(&buf))
            .await?;
        // §14.4.3, Table 405: upload responses contain at least one data byte.
        let minimum = if data.is_none() { 3 } else { 2 };
        let response = self.validate_response_length(response, minimum, None)?;
        let resp = response.data();

        // Ensure the response contains at least the block sequence counter
        if resp.is_empty() {
            return Err(self.invalid_response_length(response.payload, 2, None));
        }

        // Check block sequence counter
        if self.config.strict && resp[0] != block_sequence_counter {
            let actual = resp[0];
            return Err(
                response.deviation(ProtocolDeviation::InvalidBlockSequenceCounter {
                    expected: block_sequence_counter,
                    actual,
                }),
            );
        }

        Ok(if resp.len() > 1 {
            Some(resp[1..].to_vec())
        } else {
            None
        })
    }

    /// 0x37 - Request Transfer Exit. Used to terminate an upload or download. Has optional `data` parameter for additional information, and can optionally return additional information from the ECU. For example, this can be used to contain a checksum.
    pub async fn request_transfer_exit(&self, data: Option<&[u8]>) -> Result<Option<Vec<u8>>> {
        let resp = self
            .request(ServiceIdentifier::RequestTransferExit as u8, None, data)
            .await?;

        Ok(if !resp.is_empty() { Some(resp) } else { None })
    }
}
