//! Vector CAN Adapter support through the XL Driver
mod bindings;
pub mod error;
pub mod types;
mod vxlapi;

pub use error::Error;

use std::collections::VecDeque;

use crate::can::{AsyncCanAdapter, CanAdapter, Frame};
use crate::vector::types::{PortHandle, XLaccess, XLcanTxEvent};
use crate::vector::vxlapi::*;
use crate::Result;
use tracing::info;

#[derive(Clone)]
pub struct VectorCan {
    port_handle: PortHandle,
    channel_mask: XLaccess,
}

impl VectorCan {
    /// Convenience function to create a new adapter and wrap in an [`AsyncCanAdapter`]
    pub fn new_async(channel_idx: usize) -> Result<AsyncCanAdapter> {
        let vector = VectorCan::new(channel_idx)?;
        Ok(AsyncCanAdapter::new(vector))
    }

    /// Create a new Vector Adapter based on the global channel ID
    pub fn new(channel_idx: usize) -> Result<VectorCan> {
        xl_open_driver()?;

        // Get config based on global channel number
        let config = xl_get_driver_config(channel_idx)?;
        info!("Got Application Config: {:?}", config);

        // TODO: This produces weird errors
        // Get config based on predfined config.
        // let config = xl_get_application_config("CANalyzer", 0)?;

        let channel_mask = xl_get_channel_mask(&config)?;
        let port_handle = xl_open_port("automotive", channel_mask)?;

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

    fn config_timing(
        &mut self,
        _bus: usize,
        _config: &crate::can::timing::TimingConfig,
    ) -> crate::Result<()> {
        todo!("No yet implemented");
        Ok(())
    }
}
