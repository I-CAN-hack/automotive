//! Vector CAN Adapter support through the XL Driver
mod bindings;
pub mod error;
pub mod types;
mod vxlapi;

pub use error::Error;

use std::collections::VecDeque;

use crate::can::bitrate::{AdapterTimingConst, BitTimingConst};
use crate::can::{AsyncCanAdapter, CanAdapter, Frame};
pub use crate::vector::types::XLcanFdConf;
use crate::vector::types::{PortHandle, XLaccess, XLcanTxEvent};
use crate::vector::vxlapi::*;
use crate::Result;
use tracing::info;

/// Predefined configuration for 500 kbps arbitration bitrate and 2 Mbps data bitrate
pub const CONFIG_500K_2M_80: XLcanFdConf = XLcanFdConf {
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
        brp_max: 1024,
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
        brp_max: 1024,
        brp_inc: 1,
    }),
};

#[derive(Clone)]
pub struct VectorCan {
    port_handle: PortHandle,
    channel_mask: XLaccess,
}

impl VectorCan {
    /// Convenience function to create a new adapter and wrap in an [`AsyncCanAdapter`]
    pub fn new_async(channel_idx: usize, conf: &Option<XLcanFdConf>) -> Result<AsyncCanAdapter> {
        let vector = VectorCan::new(channel_idx, conf)?;
        Ok(AsyncCanAdapter::new(vector))
    }

    /// Create a new Vector Adapter based on the global channel ID
    /// If conf is provided, the channel will be initialized with the provided configuration.
    /// If not, the channel will be opened without requesting init (exclusive) access,
    /// and can be configured using other tools (e.g. Vector's CANalyzer).
    pub fn new(channel_idx: usize, conf: &Option<XLcanFdConf>) -> Result<VectorCan> {
        xl_open_driver()?;

        // Get config based on global channel number
        let config = xl_get_driver_config(channel_idx)?;
        info!("Got Application Config: {:?}", config);

        // TODO: This produces weird errors
        // Get config based on predfined config.
        // let config = xl_get_application_config("CANalyzer", 0)?;

        let channel_mask = xl_get_channel_mask(&config)?;
        let init_access = conf.is_some();
        let port_handle = xl_open_port("automotive", channel_mask, init_access)?;

        // Configure bitrate
        if let Some(conf) = conf {
            xl_can_fd_set_configuration(&port_handle, channel_mask, conf)?;
        }

        xl_activate_channel(&port_handle, channel_mask)?;
        info!("Connected to Vector Device. HW: {:?}", config.hw_type);

        Ok(VectorCan {
            port_handle,
            channel_mask,
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
    fn send(&mut self, frames: &mut VecDeque<Frame>) -> Result<()> {
        // TODO: can we send frames in bulk? If we fill up the TX queue can we figure out which messages were actually sent out?
        while let Some(frame) = frames.pop_front() {
            let xl_frame: XLcanTxEvent = frame.clone().into();
            let xl_frames = vec![xl_frame];

            if let Ok(tx) = xl_can_transmit_ex(&self.port_handle, self.channel_mask, &xl_frames) {
                assert_eq!(tx, 1);
            } else {
                // TODO: figure out what error happened, and decide if we can retry later or need to shut down
                frames.push_front(frame);
                break;
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
}
