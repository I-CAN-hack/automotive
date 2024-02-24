//! Unified Diagnostic Services (UDS) Client, implements ISO 14229
//! ## Example
//! ```rust
//! async fn uds_example() {
//!     let adapter = automotive::adapter::get_adapter().unwrap();
//!     let isotp = automotive::isotp::IsoTPAdapter::from_id(&adapter, 0x7a1);
//!     let uds = automotive::uds::UDSClient::new(&isotp);
//!
//!     uds.tester_present().await.unwrap();
//!     let response = uds.read_data_by_identifier(automotive::uds::constants::DataIdentifier::ApplicationSoftwareIdentification as u16).await.unwrap();
//!
//!     println!("Application Software Identification: {}", hex::encode(response));
//! }

pub mod constants;
pub mod error;
pub mod types;

use crate::error::Error;
use crate::isotp::IsoTPAdapter;
use crate::uds::constants::ServiceIdentifier;
use crate::uds::error::NegativeResponseCode;

use tokio_stream::StreamExt;
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
    pub async fn request(
        &self,
        sid: ServiceIdentifier,
        sub_function: Option<u8>,
        data: Option<&[u8]>,
    ) -> Result<Vec<u8>, Error> {
        let mut request: Vec<u8> = vec![sid as u8];

        if let Some(sub_function) = sub_function {
            request.push(sub_function);
        }

        if let Some(data) = data {
            request.extend(data);
        }

        let mut stream = self.adapter.stream();

        self.adapter.send(&request).await?;

        loop {
            let response = stream.next().await.unwrap()?;

            // Check for errors
            let response_sid = response[0];
            if response_sid == ServiceIdentifier::NegativeResponse as u8 {
                let code: NegativeResponseCode = response[2].into();

                if code == NegativeResponseCode::RequestCorrectlyReceivedResponsePending {
                    info!("Received Response Pending");
                    continue;
                }

                return Err(Error::UDSError(crate::uds::error::Error::NegativeResponse(
                    code,
                )));
            }

            // Check service id
            if response_sid != (sid as u8) | 0x40 {
                return Err(Error::UDSError(crate::uds::error::Error::InvalidServiceId(
                    response_sid,
                )));
            }

            // Check sub function
            if let Some(sub_function) = sub_function {
                if response[1] != sub_function {
                    return Err(Error::UDSError(
                        crate::uds::error::Error::InvalidSubFunction(response[1]),
                    ));
                }
            }

            let start: usize = if sub_function.is_some() { 2 } else { 1 };
            return Ok(response[start..].to_vec());
        }
    }

    /// 0x10 - Diagnostic Session Control. ECU may optionally return 4 bytes of sessionParameterRecord with some timing information.
    pub async fn diagnostic_session_control(
        &self,
        session_type: u8,
    ) -> Result<Option<types::SessionParameterRecord>, Error> {
        let result = self
            .request(
                ServiceIdentifier::DiagnosticSessionControl,
                Some(session_type),
                None,
            )
            .await?;

        let result = if result.len() == 4 {
            let p2_server_max = u16::from_be_bytes([result[0], result[1]]);
            let p2_server_max = std::time::Duration::from_millis(p2_server_max as u64);
            let p2_star_server_max = u16::from_be_bytes([result[0], result[1]]);
            let p2_star_server_max =
                std::time::Duration::from_millis(p2_star_server_max as u64 * 10);

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
    pub async fn ecu_reset(&self, reset_type: u8) -> Result<Option<u8>, Error> {
        let result = self
            .request(ServiceIdentifier::EcuReset, Some(reset_type), None)
            .await?;

        let result = if result.len() == 1 {
            Some(result[0])
        } else {
            None
        };

        Ok(result)
    }

    /// 0x27 - Security Access. Odd `access_type` values are used to request a seed, even values to send a key. The `data` parameter is optional when requesting a seed. You can use the [`constants::SecurityAccessType`] enum for the most common security level.
    pub async fn security_access(
        &self,
        access_type: u8,
        data: Option<&[u8]>,
    ) -> Result<Vec<u8>, Error> {
        let send_key = access_type % 2 == 0;
        if send_key && data.is_none() {
            panic!("Missing data parameter when sending key");
        }

        let resp = self
            .request(ServiceIdentifier::SecurityAccess, Some(access_type), data)
            .await?;

        Ok(resp)
    }

    /// 0x3E - Tester Present
    pub async fn tester_present(&self) -> Result<(), Error> {
        self.request(ServiceIdentifier::TesterPresent, Some(0), None)
            .await?;
        Ok(())
    }

    /// 0x22 - Read Data By Identifier. Specify a 16 bit data identifier, or use a constant from [`constants::DataIdentifier`] for standardized identifiers. Reading multiple identifiers simultaneously is possible on some ECUs, but not supported by this function.
    pub async fn read_data_by_identifier(&self, data_identifier: u16) -> Result<Vec<u8>, Error> {
        let did = data_identifier.to_be_bytes();
        let resp = self
            .request(ServiceIdentifier::ReadDataByIdentifier, None, Some(&did))
            .await?;

        if resp.len() < 2 {
            return Err(Error::UDSError(
                crate::uds::error::Error::InvalidResponseLength,
            ));
        }

        let did = u16::from_be_bytes([resp[0], resp[1]]);
        if did != data_identifier {
            return Err(Error::UDSError(
                crate::uds::error::Error::InvalidDataIdentifier(did),
            ));
        }

        Ok(resp[2..].to_vec())
    }

    /// 0x2E - Write Data By Identifier. Specify a 16 bit data identifier, or use a constant from [`constants::DataIdentifier`] for standardized identifiers.
    pub async fn write_data_by_identifier(
        &self,
        data_identifier: u16,
        data_record: &[u8],
    ) -> Result<(), Error> {
        let mut data: Vec<u8> = data_identifier.to_be_bytes().to_vec();
        data.extend(data_record);

        let resp = self
            .request(ServiceIdentifier::WriteDataByIdentifier, None, Some(&data))
            .await?;

        if resp.len() < 2 {
            return Err(Error::UDSError(
                crate::uds::error::Error::InvalidResponseLength,
            ));
        }

        let did = u16::from_be_bytes([resp[0], resp[1]]);
        if did != data_identifier {
            return Err(Error::UDSError(
                crate::uds::error::Error::InvalidDataIdentifier(did),
            ));
        }

        Ok(())
    }

    async fn request_download_upload(
        &self,
        sid: ServiceIdentifier,
        compression_method: u8,
        encryption_method: u8,
        memory_address: &[u8],
        memory_size: &[u8],
    ) -> Result<usize, Error> {
        assert!(
            sid == ServiceIdentifier::RequestDownload || sid == ServiceIdentifier::RequestUpload
        );
        assert!(compression_method <= 0xF);
        assert!(encryption_method <= 0xF);
        assert!(memory_address.len() > 0 && memory_address.len() <= 0xF);
        assert!(memory_size.len() > 0 && memory_size.len() <= 0xF);

        let data_format = (compression_method << 4) | encryption_method;
        let address_and_length_format =
            ((memory_size.len() as u8) << 4) | (memory_address.len() as u8);

        let mut data: Vec<u8> = vec![data_format, address_and_length_format];
        data.extend(memory_address);
        data.extend(memory_size);

        let resp = self.request(sid, None, Some(&data)).await?;

        // Ensure the response contains at least a length format
        if resp.len() == 0 {
            return Err(Error::UDSError(
                crate::uds::error::Error::InvalidResponseLength,
            ));
        }

        let num_length_bytes = (resp[0] >> 4) as usize;
        if num_length_bytes == 0 || num_length_bytes > 8 || resp.len() != num_length_bytes + 1 {
            return Err(Error::UDSError(
                crate::uds::error::Error::InvalidResponseLength,
            ));
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
    ) -> Result<usize, Error> {
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
    ) -> Result<usize, Error> {
        self.request_download_upload(
            ServiceIdentifier::RequestUpload,
            compression_method,
            encryption_method,
            memory_address,
            memory_size,
        )
        .await
    }
}
