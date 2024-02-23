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

    /// 0x3E - Tester Present
    pub async fn tester_present(&self) -> Result<(), Error> {
        self.request(ServiceIdentifier::TesterPresent, Some(0), None)
            .await?;
        Ok(())
    }

    /// 0x22 - Read Data By Identifier. Specify a 16 bit data identifier, or use a constant from [`constants::DataIdentifier`] for standardized identifiers. Reading multiple identifiers is not supported.
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
}
