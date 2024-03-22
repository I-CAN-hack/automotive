pub mod bit_timing;
mod error;
pub mod types;
pub mod wrapper;

pub use error::Error;

use bit_timing::BitTimingKind;
use types::CanFilter;

use crate::{XLaccess, XL_BUS_TYPE_CAN, XL_INTERFACE_VERSION, XL_INTERFACE_VERSION_V4};
use std::collections::HashMap;

pub struct VectorCan {
    pub channels: Vec<u32>,
    pub can_filters: Option<Vec<CanFilter>>,
    pub poll_interval: f32,
    pub receive_own_messages: bool,
    pub timing: Option<BitTimingKind>,
    pub rx_queue_size: u32,
    pub app_name: String,
    pub serial: Option<u32>,
    pub fd_mode: bool,
    pub bit_rate: Option<u32>,
    port_handle: i32,
}

impl Default for VectorCan {
    fn default() -> Self {
        Self {
            app_name: String::from("AutomotiveVectorAnalyzer"),
            channels: Vec::new(),
            can_filters: None,
            poll_interval: 0.01,
            receive_own_messages: false,
            timing: None,
            rx_queue_size: 2_u32.pow(14),
            serial: None,
            fd_mode: false,
            bit_rate: None,
            port_handle: -1,
        }
    }
}

impl VectorCan {
    pub fn new(
        channels: Vec<u32>,
        can_filters: Option<Vec<CanFilter>>,
        poll_interval: f32,
        receive_own_messages: bool,
        timing: Option<BitTimingKind>,
        rx_queue_size: u32,
        app_name: String,
        serial: Option<u32>,
        fd_mode: bool,
        bit_rate: Option<u32>,
    ) {
        let channel_configs = wrapper::get_channel_configs().unwrap();
        let mut mask: u64 = 0;
        let mut channel_masks: HashMap<u32, u64> = HashMap::new();
        let mut index_to_channel: HashMap<u32, u32> = HashMap::new();

        for channel in channels {
            let channel_index =
                wrapper::find_global_channel_idx(channel as u8, serial, Some(&app_name), channel_configs.clone())
                    .unwrap();
            let channel_mask: u64 = 1 << channel_index;
            channel_masks.insert(channel, channel_mask);
            index_to_channel.insert(channel_index as u32, channel);
            mask |= channel_mask;
        }

        let mut permission_mask: Option<XLaccess> = None;//XLaccess::default();
        if bit_rate.is_some() || fd_mode {
            permission_mask = Some(mask);
        }

        let inetface_version = match fd_mode {
            true => XL_INTERFACE_VERSION_V4,
            false => XL_INTERFACE_VERSION,
        };

        let port_config =
            wrapper::open_port(&app_name, mask, permission_mask, rx_queue_size, inetface_version, XL_BUS_TYPE_CAN).unwrap();

        // TODO: Implement check_can_settings
        let assert_timing = bit_rate.is_some() || timing.is_some();

        if let Some(timing) = timing {
            match timing {
                BitTimingKind::Standard(timing) => {
                    wrapper::set_bit_timing(port_config.port_handle, mask, port_config.permission_mask, &timing).unwrap();
                    // let bit_rate = bit_rate.unwrap_or(500_000);
                    // let bit_timing = bit_timing::BitTiming::new(bit_rate, timing).unwrap();
                    // wrapper::set_bit_timing(port_handle, channel_masks, bit_timing).unwrap();
                },
                BitTimingKind::Extended(timing) => {
                    wrapper::set_bit_timing_fd(port_config.port_handle, mask, port_config.permission_mask, &timing).unwrap();
                }
            }
        }
    }
}
