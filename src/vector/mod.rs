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
    pub fn new_async() -> Result<AsyncCanAdapter> {
        let vector = VectorCan::new()?;
        Ok(AsyncCanAdapter::new(vector))
    }

    pub fn new() -> Result<VectorCan> {
        xl_open_driver()?;

        // Get config based on global channel number
        let config = xl_get_driver_config(0)?;

        // Get config based on predfined config.
        // TODO: This produces weird errors
        // let config = xl_get_application_config("CANalyzer", 0)?;

        info!("Got Application Config: {:?}", config);

        // let channel_index = xl_get_channel_index(&config)?;
        // info!("Channel index: {}", channel_index);

        let channel_mask = xl_get_channel_mask(&config)?;
        // info!("Channel mask: {}", channel_mask);

        let port_handle = xl_open_port("automotive", channel_mask)?;
        // info!("Port: {:?}", port_handle);

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
        // TODO: can we send frames in bulk? If we fill up the TXqueue we need to know which messages were actually sent out
        while let Some(frame) = frames.pop_front() {
            let xl_frame: XLcanTxEvent = frame.clone().into();
            let xl_frames = vec![xl_frame];

            if let Ok(tx) = xl_can_transmit_ex(&self.port_handle, self.channel_mask, &xl_frames) {
                assert_eq!(tx, 1);
            } else {
                // TODO: figure out what error happened, and decide if we can retry later
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
}
