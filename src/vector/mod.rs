//! Vector CAN Adapter support through the XL Driver
mod bindings;
mod config;
pub mod error;
pub mod types;
mod vxlapi;

pub use config::{VectorAccess, VectorConfig, VectorConfigBuilder};
pub use error::Error;

use std::collections::VecDeque;

use crate::can::adapter::validate_tx_timeout;
use crate::can::bitrate::{AdapterTimingConst, BitTimingConst};
use crate::can::{AsyncCanAdapter, CanAdapter, Frame};
use crate::vector::bindings::XLcanFdConf;
use crate::vector::types::{PortHandle, XLaccess, XLcanTxEvent};
use crate::vector::vxlapi::*;
use crate::Result;
use tracing::info;

/// Vector XL timing capabilities used for bitrate helper calculations.
pub const VECTOR_TIMING_CONST: AdapterTimingConst = AdapterTimingConst {
    nominal: BitTimingConst {
        clock_hz: 80_000_000,
        tseg1_min: 2,
        tseg1_max: 254,
        tseg2_min: 2,
        tseg2_max: 254,
        sjw_max: 128,
        brp_min: 1,
        brp_max: 64,
        brp_inc: 1,
    },
    data: Some(BitTimingConst {
        clock_hz: 80_000_000,
        tseg1_min: 2,
        tseg1_max: 126,
        tseg2_min: 2,
        tseg2_max: 126,
        sjw_max: 64,
        brp_min: 1,
        brp_max: 64,
        brp_inc: 1,
    }),
};

#[derive(Clone)]
pub struct VectorCan {
    port_handle: PortHandle,
    channel_mask: XLaccess,
    tx_timeout: Option<std::time::Duration>,
    replace_pending: bool,
}

impl VectorCan {
    /// Convenience function to create a new adapter and wrap in an [`AsyncCanAdapter`]
    pub fn new_async(config: VectorConfig) -> Result<AsyncCanAdapter> {
        let vector = VectorCan::new(config)?;
        Ok(AsyncCanAdapter::new(vector))
    }

    /// Open a shared Vector channel or request initialization access and
    /// configure its bus timing. Shared channels retain their existing timing.
    pub fn new(config: VectorConfig) -> Result<VectorCan> {
        validate_tx_timeout(config.tx_timeout)?;
        let tx_timeout = config.tx_timeout;
        let conf = match config.access {
            VectorAccess::Shared => None,
            VectorAccess::Init { bitrate } => Some(bitrate),
        };
        if conf.as_ref().is_some_and(|conf| conf.data.is_none()) {
            return Err(crate::Error::InvalidBitrate(
                "Vector bitrate configuration requires a CAN-FD data phase".to_string(),
            ));
        }

        xl_open_driver()?;

        // Get config based on global channel number
        let config = xl_get_driver_config(config.channel)?;
        info!("Got Application Config: {:?}", config);

        // TODO: This produces weird errors
        // Get config based on predfined config.
        // let config = xl_get_application_config("CANalyzer", 0)?;

        let channel_mask = xl_get_channel_mask(&config)?;
        let init_access = conf.is_some();
        let port_handle = xl_open_port("automotive", channel_mask, init_access)?;

        // Configure bitrate
        if let Some(conf) = conf {
            let conf: XLcanFdConf = conf.into();
            xl_can_fd_set_configuration(&port_handle, channel_mask, &conf)?;
        }

        xl_activate_channel(&port_handle, channel_mask)?;
        info!("Connected to Vector Device. HW: {:?}", config.hw_type);

        Ok(VectorCan {
            port_handle,
            channel_mask,
            tx_timeout,
            // Always clear transmissions left by an earlier session on the
            // first real send, including when the timeout is disabled.
            replace_pending: true,
        })
    }
}

impl Drop for VectorCan {
    fn drop(&mut self) {
        info!("Closing Vector Device");
        xl_deactivate_channel(&self.port_handle, self.channel_mask).unwrap();
        xl_close_port(&self.port_handle).unwrap();
        xl_close_driver().unwrap();
    }
}

impl CanAdapter for VectorCan {
    fn supports_tx_timeout() -> bool {
        true
    }

    fn tx_timeout(&self) -> Option<std::time::Duration> {
        self.tx_timeout
    }

    fn cancel_pending_tx(&mut self) -> Result<()> {
        // xlCanFlushTransmitQueue is a successful no-op on newer devices.
        // HIGHPRIO clears the hardware queues before the next real frame.
        self.replace_pending = true;
        Ok(())
    }

    fn send(&mut self, frames: &mut VecDeque<Frame>) -> Result<()> {
        while let Some(frame) = frames.pop_front() {
            let mut xl_frame = match XLcanTxEvent::try_from(&frame) {
                Ok(frame) => frame,
                Err(error) => {
                    frames.push_front(frame);
                    return Err(error);
                }
            };
            if self.replace_pending {
                unsafe {
                    xl_frame.tagData.canMsg.msgFlags |= bindings::XL_CAN_TXMSG_FLAG_HIGHPRIO;
                }
            }
            match xl_can_transmit_ex(
                &self.port_handle,
                self.channel_mask,
                std::slice::from_ref(&xl_frame),
            ) {
                Ok(1) => self.replace_pending = false,
                Ok(_) => {
                    frames.push_front(frame);
                    break;
                }
                Err(error) => {
                    frames.push_front(frame);
                    return Err(error);
                }
            }
        }

        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<Frame>> {
        let mut frames = vec![];

        while let Some(frame) = xl_can_receive(&self.port_handle)? {
            if let Ok(frame) = frame.try_into() {
                frames.push(frame);
            }
        }

        Ok(frames)
    }

    fn timing_const() -> crate::can::bitrate::AdapterTimingConst
    where
        Self: Sized,
    {
        VECTOR_TIMING_CONST
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::can::bitrate::BitrateBuilder;

    const CONFIG_500K_2M_80: XLcanFdConf = XLcanFdConf {
        arbitrationBitRate: 500_000,
        sjwAbr: 1,
        tseg1Abr: 127,
        tseg2Abr: 32,
        dataBitRate: 2_000_000,
        sjwDbr: 1,
        tseg1Dbr: 31,
        tseg2Dbr: 8,
        reserved: 0,
        options: 0,
        reserved1: [0, 0],
        reserved2: 0,
    };

    #[test]
    fn bitrate_builder_matches_predefined_config() {
        let bitrate_cfg = BitrateBuilder::new::<VectorCan>()
            .bitrate(500_000)
            .sample_point(0.8)
            .sjw(1)
            .data_bitrate(2_000_000)
            .data_sample_point(0.8)
            .data_sjw(1)
            .build()
            .unwrap();

        let conf: XLcanFdConf = bitrate_cfg.into();
        assert_eq!(conf, CONFIG_500K_2M_80);
    }

    #[test]
    fn rejects_nominal_only_bitrate_config() {
        let config = VectorConfig::builder(0)
            .bitrate(|bitrate| bitrate.bitrate(500_000).sample_point(0.8).sjw(1))
            .build()
            .unwrap();

        match VectorCan::new(config) {
            Err(crate::Error::InvalidBitrate(msg)) => {
                assert_eq!(
                    msg,
                    "Vector bitrate configuration requires a CAN-FD data phase"
                );
            }
            Err(err) => panic!("unexpected error: {err}"),
            Ok(_) => panic!("expected invalid bitrate error"),
        }
    }
}
