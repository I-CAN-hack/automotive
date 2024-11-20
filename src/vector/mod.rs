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

        // TODO: Accept app name and channel from user
        let config = xl_get_application_config("CANalyzer", 0)?;

        // let config = xl_get_application_config("NewApp", 0)?;
        // info!("Got Application Config: {:?}", config);

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
        if frames.is_empty() {
            return Ok(());
        }

        let frames: Vec<Frame> = frames.drain(..).collect();
        let frames: Vec<XLcanTxEvent> = frames.into_iter().map(|f| f.into()).collect();

        let tx = xl_can_transmit_ex(&self.port_handle, self.channel_mask, &frames)?;
        assert_eq!(tx, frames.len() as u32);

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
