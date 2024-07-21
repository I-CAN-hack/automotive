//! Unified Diagnostic Services (UDS) Client, implements ISO 14229
//! ## Example
//! ```rust
//! async fn uds_example() {
//!     let adapter = automotive::can::get_adapter().unwrap();
//!     let isotp = automotive::isotp::IsoTPAdapter::from_id(&adapter, 0x7a1);
//!     let uds = automotive::uds::UDSClient::new(&isotp);
//!
//!     uds.tester_present().await.unwrap();
//!     let response = uds.read_data_by_identifier(automotive::uds::DataIdentifier::ApplicationSoftwareIdentification as u16).await.unwrap();
//!
//!     println!("Application Software Identification: {}", hex::encode(response));
//! }

mod constants;
mod error;
mod types;

use crate::isotp::IsoTPAdapter;
use crate::Result;
use crate::StreamExt;
pub use constants::*;
pub use error::{Error, NegativeResponseCode};
pub use types::*;

use tracing::info;

/// UDS Client. Wraps an IsoTPAdapter to provide a simple interface for making UDS calls.
pub struct UDSClient<'a> {
    adapter: &'a IsoTPAdapter<'a>,
}

impl<'a> UDSClient<'a> {
    pub fn new(adapter: &'a IsoTPAdapter) -> Self {
        Self { adapter }
    }

    /// Helper function to make custom UDS requests. This function will verify the ECU responds with the correct service identifier and sub function, handle negative responses, and will return the response data.
    pub async fn request(&self, sid: u8, sub_function: Option<u8>, data: Option<&[u8]>) -> Result<Vec<u8>> {
        let mut request: Vec<u8> = vec![sid];

        if let Some(sub_function) = sub_function {
            request.push(sub_function);
        }

        if let Some(data) = data {
            request.extend(data);
        }

        let mut stream = self.adapter.recv();

        self.adapter.send(&request).await?;

        loop {
            let response = stream.next().await.unwrap()?;

            // Check for errors
            let response_sid = response[0];
            if response_sid == NEGATIVE_RESPONSE {
                let code: NegativeResponseCode = response[2].into();

                if code == NegativeResponseCode::RequestCorrectlyReceivedResponsePending {
                    info!("Received Response Pending");
                    continue;
                }

                return Err(Error::NegativeResponse(code).into());
            }

            // Check service id
            if response_sid != sid | POSITIVE_RESPONSE {
                return Err(Error::InvalidServiceId(response_sid).into());
            }

            // Check sub function
            if let Some(sub_function) = sub_function {
                if response[1] != sub_function {
                    return Err(Error::InvalidSubFunction(response[1]).into());
                }
            }

            let start: usize = if sub_function.is_some() { 2 } else { 1 };
            return Ok(response[start..].to_vec());
        }
    }

    /// 0x10 - Diagnostic Session Control. ECU may optionally return 4 bytes of sessionParameterRecord with some timing information.
    pub async fn diagnostic_session_control(&self, session_type: u8) -> Result<Option<types::SessionParameterRecord>> {
        let result = self
            .request(
                ServiceIdentifier::DiagnosticSessionControl as u8,
                Some(session_type),
                None,
            )
            .await?;

        let result = if result.len() == 4 {
            let p2_server_max = u16::from_be_bytes([result[0], result[1]]);
            let p2_server_max = std::time::Duration::from_millis(p2_server_max as u64);
            let p2_star_server_max = u16::from_be_bytes([result[0], result[1]]);
            let p2_star_server_max = std::time::Duration::from_millis(p2_star_server_max as u64 * 10);

            Some(types::SessionParameterRecord {
                p2_server_max,
                p2_star_server_max,
            })
        } else {
            None
        };

        Ok(result)
    }

    /// 0x11 - ECU Reset. The `reset_type` parameter can be used to specify the type of reset to perform. Use the [`constants::ResetType`] enum for  the reset types defined in the standard. This function returns the power down time when the reset type is [`constants::ResetType::EnableRapidPowerShutDown`].
    pub async fn ecu_reset(&self, reset_type: u8) -> Result<Option<u8>> {
        let result = self
            .request(ServiceIdentifier::EcuReset as u8, Some(reset_type), None)
            .await?;

        let result = if result.len() == 1 { Some(result[0]) } else { None };

        Ok(result)
    }

    /// 0x27 - Security Access. Odd `access_type` values are used to request a seed, even values to send a key. The `data` parameter is optional when requesting a seed. You can use the [`constants::SecurityAccessType`] enum for the default security level.
    pub async fn security_access(&self, access_type: u8, data: Option<&[u8]>) -> Result<Vec<u8>> {
        let send_key = access_type % 2 == 0;
        if send_key && data.is_none() {
            panic!("Missing data parameter when sending key");
        }

        let resp = self
            .request(ServiceIdentifier::SecurityAccess as u8, Some(access_type), data)
            .await?;

        Ok(resp)
    }

    /// 0x3E - Tester Present
    pub async fn tester_present(&self) -> Result<()> {
        self.request(ServiceIdentifier::TesterPresent as u8, Some(0), None)
            .await?;
        Ok(())
    }

    async fn read_write_memory_by_adddress(
        &self,
        sid: ServiceIdentifier,
        memory_address: &[u8],
        memory_size: &[u8],
        data: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        assert!(sid == ServiceIdentifier::ReadMemoryByAddress || sid == ServiceIdentifier::WriteMemoryByAddress);
        assert!(!memory_address.is_empty() && memory_address.len() <= 0xF);
        assert!(!memory_size.is_empty() && memory_size.len() <= 0xF);

        let address_and_length_format = ((memory_size.len() as u8) << 4) | (memory_address.len() as u8);

        let mut buf: Vec<u8> = vec![address_and_length_format];
        buf.extend(memory_address);
        buf.extend(memory_size);
        if let Some(data) = data {
            buf.extend(data);
        }

        self.request(sid as u8, None, Some(&buf)).await
    }

    /// 0x22 - Read Data By Identifier. Specify a 16 bit data identifier, or use a constant from [`constants::DataIdentifier`] for standardized identifiers. Reading multiple identifiers simultaneously is possible on some ECUs, but not supported by this function.
    pub async fn read_data_by_identifier(&self, data_identifier: u16) -> Result<Vec<u8>> {
        let did = data_identifier.to_be_bytes();
        let resp = self
            .request(ServiceIdentifier::ReadDataByIdentifier as u8, None, Some(&did))
            .await?;

        if resp.len() < 2 {
            return Err(Error::InvalidResponseLength.into());
        }

        let did = u16::from_be_bytes([resp[0], resp[1]]);
        if did != data_identifier {
            return Err(Error::InvalidDataIdentifier(did).into());
        }

        Ok(resp[2..].to_vec())
    }

    /// 0x23 - Read Memory By Address. The `memory_address` parameter should be the address to read from, and the `memory_size` parameter should be the number of bytes to read.
    pub async fn read_memory_by_address(&self, memory_address: &[u8], memory_size: &[u8]) -> Result<Vec<u8>> {
        self.read_write_memory_by_adddress(
            ServiceIdentifier::ReadMemoryByAddress,
            memory_address,
            memory_size,
            None,
        )
        .await
    }

    /// 0x2E - Write Data By Identifier. Specify a 16 bit data identifier, or use a constant from [`constants::DataIdentifier`] for standardized identifiers.
    pub async fn write_data_by_identifier(&self, data_identifier: u16, data_record: &[u8]) -> Result<()> {
        let mut data: Vec<u8> = data_identifier.to_be_bytes().to_vec();
        data.extend(data_record);

        let resp = self
            .request(ServiceIdentifier::WriteDataByIdentifier as u8, None, Some(&data))
            .await?;

        if resp.len() < 2 {
            return Err(Error::InvalidResponseLength.into());
        }

        let did = u16::from_be_bytes([resp[0], resp[1]]);
        if did != data_identifier {
            return Err(Error::InvalidDataIdentifier(did).into());
        }

        Ok(())
    }

    /// 0x3D - Write Memory By Address. The `memory_address` parameter should be the address to write to, and the `memory_size` parameter should be the number of bytes to write. The `data` parameter should be the data to write.
    pub async fn write_memory_by_address(&self, memory_address: &[u8], memory_size: &[u8], data: &[u8]) -> Result<()> {
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
        let resp = self
            .request(
                ServiceIdentifier::ReadDTCInformation as u8,
                Some(ReportType::ReportNumberOfDTCByStatusMask as u8),
                Some(&[mask]),
            )
            .await?;

        if resp.len() != 4 {
            return Err(Error::InvalidResponseLength.into());
        }

        let mask = resp[0];
        let format =
            DTCFormatIdentifier::from_repr(resp[1]).expect("Unknown DTC Format Identifier");
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

        let resp = self
            .request(
                ServiceIdentifier::RoutineControl as u8,
                Some(routine_control_type as u8),
                Some(&buf),
            )
            .await?;

        if resp.len() < 2 {
            return Err(Error::InvalidResponseLength.into());
        }

        let id = u16::from_be_bytes([resp[0], resp[1]]);
        if id != routine_identifier {
            return Err(Error::InvalidDataIdentifier(id).into());
        }

        Ok(if resp.len() > 2 { Some(resp[2..].to_vec()) } else { None })
    }

    async fn request_download_upload(
        &self,
        sid: ServiceIdentifier,
        compression_method: u8,
        encryption_method: u8,
        memory_address: &[u8],
        memory_size: &[u8],
    ) -> Result<usize> {
        assert!(sid == ServiceIdentifier::RequestDownload || sid == ServiceIdentifier::RequestUpload);
        assert!(compression_method <= 0xF);
        assert!(encryption_method <= 0xF);
        assert!(!memory_address.is_empty() && memory_address.len() <= 0xF);
        assert!(!memory_size.is_empty() && memory_size.len() <= 0xF);

        let data_format = (compression_method << 4) | encryption_method;
        let address_and_length_format = ((memory_size.len() as u8) << 4) | (memory_address.len() as u8);

        let mut data: Vec<u8> = vec![data_format, address_and_length_format];
        data.extend(memory_address);
        data.extend(memory_size);

        let resp = self.request(sid as u8, None, Some(&data)).await?;

        // Ensure the response contains at least a length format
        if resp.is_empty() {
            return Err(Error::InvalidResponseLength.into());
        }

        let num_length_bytes = (resp[0] >> 4) as usize;
        if num_length_bytes == 0 || num_length_bytes > 8 || resp.len() != num_length_bytes + 1 {
            return Err(Error::InvalidResponseLength.into());
        }

        // Convert the length bytes to a usize
        let length = resp[1..num_length_bytes + 1]
            .iter()
            .fold(0, |acc, &x| (acc << 8) | x as usize);

        Ok(length)
    }

    /// 0x34 - Request Download. Used to initiate a transfer from the client to the ECU. Returns the maximum number of bytes to include in each TransferData request.
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

    /// 0x35 - Request Upload. Used to initiate a transfer from the client to the ECU. Returns the maximum number of bytes to include in each TransferData request.
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
    pub async fn transfer_data(&self, block_sequence_counter: u8, data: Option<&[u8]>) -> Result<Option<Vec<u8>>> {
        let mut buf: Vec<u8> = vec![block_sequence_counter];
        if let Some(data) = data {
            buf.extend(data);
        }

        let resp = self
            .request(ServiceIdentifier::TransferData as u8, None, Some(&buf))
            .await?;

        // Ensure the response contains at least the block sequence counter
        if resp.is_empty() {
            return Err(Error::InvalidResponseLength.into());
        }

        // Check block sequence counter
        if resp[0] != block_sequence_counter {
            return Err(Error::InvalidBlockSequenceCounter(resp[0]).into());
        }

        Ok(if resp.len() > 1 { Some(resp[1..].to_vec()) } else { None })
    }

    /// 0x37 - Request Transfer Exit. Used to terminate an upload or download. Has optional `data` parameter for additional information, and can optionally return additional information from the ECU. For example, this can be used to contain a checksum.
    pub async fn request_transfer_exit(&self, data: Option<&[u8]>) -> Result<Option<Vec<u8>>> {
        let resp = self
            .request(ServiceIdentifier::RequestTransferExit as u8, None, data)
            .await?;

        Ok(if !resp.is_empty() { Some(resp) } else { None })
    }
}
