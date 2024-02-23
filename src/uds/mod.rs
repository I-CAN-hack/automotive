//! Unified Diagnostic Services (UDS) Client, implements ISO 14229
//! ## Example
//! ```
//! let adapter = automotive::adapter::get_adapter().unwrap();
//! let isotp = automotive::isotp::IsoTPAdapter::from_id(&adapter, 0x7a1);
//! let uds = automotive::uds::UDSClient::new(&isotp);
//!
//! uds.tester_present().await.unwrap();
//! let response = uds.read_data_by_identifier(DataIdentifier::ApplicationSoftwareIdentification as u16).await.unwrap();
//!
//! println!("Application Software Identification: {}", hex::encode(response));
//! ```

pub mod constants;
pub mod error;

use crate::error::Error;
use crate::isotp::IsoTPAdapter;
use crate::uds::constants::ServiceIdentifier;
use crate::uds::error::NegativeResponseCode;

use tracing::debug;

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
        let response = self.adapter.recv();
        let mut request: Vec<u8> = vec![sid as u8];

        if let Some(sub_function) = sub_function {
            request.push(sub_function);
        }

        if let Some(data) = data {
            request.extend(data);
        }

        debug!("TX {}", hex::encode(&request));
        self.adapter.send(&request).await?;

        let response = response.await?;
        debug!("RX {}", hex::encode(&request));

        // Check for errors
        let response_sid = response[0];
        if response_sid == ServiceIdentifier::NegativeResponse as u8 {
            let code: NegativeResponseCode = response[2].into();

            // TODO: handle response pending

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
        Ok(response[start..].to_vec())
    }

    /// 0x3E - Tester Present
    pub async fn tester_present(&self) -> Result<(), Error> {
        self.request(ServiceIdentifier::TesterPresent, Some(0), None).await?;
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
