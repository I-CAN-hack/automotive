pub mod bit_timing;
mod error;
pub mod types;
pub mod wrapper;

pub use error::Error;

use bit_timing::BitTimingKind;
use types::CanFilter;

use crate::can::{AsyncCanAdapter, CanAdapter, Frame, Identifier};
use crate::{
    e_XLevent_type_XL_TRANSMIT_MSG, s_xl_can_msg, s_xl_tag_data, xlCanSetChannelMode, XLaccess,
    XLcanTxEvent, XLcanTxEvent__bindgen_ty_1, XLevent, XLhandle, XLportHandle, XL_BUS_TYPE_CAN,
    XL_CAN_EXT_MSG_ID, XL_CAN_MSG_FLAG_TX_COMPLETED, XL_CAN_TXMSG_FLAG_EDL, XL_CAN_TX_MSG,
    XL_INTERFACE_VERSION, XL_INTERFACE_VERSION_V4,
};
use std::collections::HashMap;
use std::collections::VecDeque;

const XL_CAN_EV_TAG_TX_MSG: u16 = 1088;

const CAN_FRAME_DATA_LENGTH: usize = 8;

#[derive(Clone)]
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
    pub port_handle: XLportHandle,
    // event_handle: XLhandle,
    mask: XLaccess,
    permission_mask: XLaccess,
    loopback_queue: VecDeque<Frame>,
}

impl Default for VectorCan {
    fn default() -> Self {
        Self::new(
            Vec::new(),
            None,
            0.01,
            false,
            None,
            //2_u32.pow(14),
            8192,
            String::from("CANalyzer"),
            None,
            false,
            None,
            // -1,
            // event_handle: std::ptr::null_mut(),
            // mask: 0,
            // permission_mask: 0,
        )
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
    ) -> Self {
        /*

        let channel_configs = wrapper::get_channel_configs().unwrap();
        let mut mask: u64 = 0;
        let mut channel_masks: HashMap<u32, u64> = HashMap::new();
        let mut index_to_channel: HashMap<u32, u32> = HashMap::new();

        for channel in &channels {
            let channel_index =
                wrapper::find_global_channel_idx(*channel as u8, serial, Some(&app_name), channel_configs.clone())
                    .unwrap();
            let channel_mask: u64 = 1 << channel_index;
            channel_masks.insert(*channel, channel_mask);
            index_to_channel.insert(channel_index as u32, *channel);
            mask |= channel_mask;
        }

        let mut permission_mask: Option<XLaccess> = None; //XLaccess::default();
        if bit_rate.is_some() || fd_mode {
            permission_mask = Some(mask);
        }

        let inetface_version = match fd_mode {
            true => XL_INTERFACE_VERSION_V4,
            false => XL_INTERFACE_VERSION,
        };

        */
        let inetface_version = XL_INTERFACE_VERSION;

        //let mask: XLaccess = 0b0001100; // CAN bus is on channel 3 and 4
        let mask: XLaccess = 0b00001000;
        let permission_mask: Option<XLaccess> = Some(mask.clone()); //XLaccess::default();

        let port_config = wrapper::open_port(
            &app_name,
            mask,
            permission_mask,
            rx_queue_size,
            inetface_version,
            XL_BUS_TYPE_CAN,
        )
        .unwrap();

        // TODO: Implement check_can_settings
        let assert_timing = bit_rate.is_some() || timing.is_some();

        if let Some(timing) = &timing {
            match timing {
                BitTimingKind::Standard(timing) => {
                    wrapper::set_bit_timing(
                        port_config.port_handle,
                        mask,
                        port_config.permission_mask,
                        &timing,
                    )
                    .unwrap();
                    // let bit_rate = bit_rate.unwrap_or(500_000);
                    // let bit_timing = bit_timing::BitTiming::new(bit_rate, timing).unwrap();
                    // wrapper::set_bit_timing(port_handle, channel_masks, bit_timing).unwrap();
                }
                BitTimingKind::Extended(timing) => {
                    wrapper::set_bit_timing_fd(
                        port_config.port_handle,
                        mask,
                        port_config.permission_mask,
                        &timing,
                    )
                    .unwrap();
                }
            }
        } else if fd_mode {
            // TODO: Implement https://github.com/hardbyte/python-can/blob/4a41409de8e1eefaa1aa003da7e4f84f018c6791/can/interfaces/vector/canlib.py#L288
        } else if let Some(bit_rate) = bit_rate {
            wrapper::set_bit_rate(
                port_config.port_handle,
                mask,
                port_config.permission_mask,
                bit_rate,
            )
            .unwrap();
        } else {
            println!("We are setting the to the default bit rate of 5000000");
            wrapper::set_bit_rate(
                port_config.port_handle,
                mask,
                port_config.permission_mask,
                500_000,
            )
            .unwrap();
        }

        unsafe {
            let status = xlCanSetChannelMode(port_config.port_handle, mask, 0x0, 0x0);

            println!("Set channel status: {:?}", status);
        };

        let mut event_handle: XLhandle = std::ptr::null_mut();
        wrapper::set_notification(port_config.port_handle, &mut event_handle, 1).unwrap();

        match wrapper::activate_channel(port_config.port_handle, mask, XL_BUS_TYPE_CAN, 0) {
            Ok(_) => {}
            Err(e) => {
                wrapper::deactivate_channel(port_config.port_handle, mask).unwrap();
                wrapper::close_port(port_config.port_handle).unwrap();
                wrapper::close_driver().unwrap();
                panic!("Error activating channel: {:?}", e);
            }
        };

        Self {
            channels,
            can_filters,
            poll_interval,
            receive_own_messages,
            timing,
            rx_queue_size,
            app_name,
            serial,
            fd_mode,
            bit_rate,
            port_handle: port_config.port_handle,
            // event_handle: event_handle,
            mask,
            permission_mask: port_config.permission_mask,
            loopback_queue: VecDeque::new(),
        }
    }

    pub fn shutdown(&self) {
        wrapper::deactivate_channel(self.port_handle, self.mask).unwrap();
        wrapper::close_port(self.port_handle).unwrap();
        wrapper::close_driver().unwrap();
    }

    pub fn new_async(&self) -> AsyncCanAdapter {
        // let socket = SocketCan::new(socket);

        AsyncCanAdapter::new(self.clone())
    }

    fn get_tx_channel_mask(&self, frames: &VecDeque<Frame>) -> u64 {
        //if frames.len() == 1 {
        //  self.channel_masks.get(&(frames[0].bus as u32)).unwrap_or(&self.mask).clone()
        //} else {
        //  self.mask
        //}
        self.mask
    }

    fn send_can(&self, frames: &VecDeque<Frame>) -> Result<u32, Error> {
        let mask = self.get_tx_channel_mask(frames);

        // Change to send the frames one at a time so we can retry them
        let mut events = vec![];
        for frame in frames {
            events.push(build_xl_event(frame));
        }

        wrapper::send_can(self.port_handle, mask, events.len() as u32, events)
    }

    fn receive_can(&self) -> Result<Frame, Error> {
        let event = match wrapper::receive_can(self.port_handle) {
            Ok(event) => event,
            Err(err) => return Err(err),
        };

        // Ok(Frame::from(event))
        Ok(event.into())
    }

    fn send_can_fd(&self, frames: &VecDeque<Frame>) {
        let mask = self.get_tx_channel_mask(frames);

        let mut events = vec![];
        for frame in frames {
            events.push(build_xl_can_tx_event(frame));
        }

        wrapper::send_can_fd(self.port_handle, mask, events.len(), events);
    }
}

impl CanAdapter for VectorCan {
    fn send(&mut self, frames: &mut VecDeque<Frame>) -> Result<(), crate::error::Error> {
        // let mask = self.get_tx_channel_mask(frames);
        match self.fd_mode {
            true => {
                self.send_can_fd(frames);
            }
            false => {
                self.send_can(frames)?;
            }
        };

        for frame in frames {
            let mut frame = frame.clone();
            frame.loopback = true;
            self.loopback_queue.push_back(frame);
        }

        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<Frame>, crate::error::Error> {
        let frame = match self.receive_can() {
            Ok(frame) => frame,
            Err(err) => match err {
                Error::EmptyQueue => return Ok(vec![]),
                _ => return Err(crate::error::Error::VectorError(err)),
            },
        };

        let mut frames = vec![frame];

        // Add fake loopback frames to the receive queue
        frames.extend(self.loopback_queue.drain(..));

        Ok(frames)
    }
}

fn is_loopback(event: &XLevent) -> bool {
    unsafe { (event.tagData.msg.flags & XL_CAN_MSG_FLAG_TX_COMPLETED as u16) == 0 }
}

fn build_xl_event(frame: &Frame) -> XLevent {
    let id = match frame.id {
        Identifier::Standard(id) => id,
        Identifier::Extended(id) => id | XL_CAN_EXT_MSG_ID,
    };

    let data: [u8; CAN_FRAME_DATA_LENGTH] = {
        let mut array = [0u8; CAN_FRAME_DATA_LENGTH];
        array[..CAN_FRAME_DATA_LENGTH].copy_from_slice(&frame.data[..CAN_FRAME_DATA_LENGTH]);

        array
    };

    let event = XLevent {
        tag: e_XLevent_type_XL_TRANSMIT_MSG as u8,
        chanIndex: 0, //frame.bus,
        transId: 0,
        portHandle: 0,
        flags: 0,
        reserved: 0,
        timeStamp: 0,
        tagData: s_xl_tag_data {
            msg: s_xl_can_msg {
                id,
                flags: 0,
                dlc: CAN_FRAME_DATA_LENGTH as u16,
                res1: 0,
                data,
                res2: 0,
            },
        },
    };

    event
}

fn build_xl_can_tx_event(frame: &Frame) -> XLcanTxEvent {
    let id = match frame.id {
        Identifier::Standard(id) => id,
        Identifier::Extended(id) => id | XL_CAN_EXT_MSG_ID,
    };

    let mut flags = 0;
    if frame.fd {
        flags |= XL_CAN_TXMSG_FLAG_EDL;
    }

    let event = XLcanTxEvent {
        tag: XL_CAN_EV_TAG_TX_MSG,
        transId: 0xFFFF,
        channelIndex: frame.bus as u8,
        reserved: [0, 0, 0],
        tagData: XLcanTxEvent__bindgen_ty_1 {
            canMsg: XL_CAN_TX_MSG {
                canId: id,
                msgFlags: flags,
                dlc: 0,
                reserved: [0, 0, 0, 0, 0, 0, 0],
                data: [0; 64],
            },
        },
    };

    event
}

impl From<XLevent> for Frame {
    fn from(event: XLevent) -> Self {
        let data = unsafe { event.tagData.msg.data.to_vec() };

        let tx_id = unsafe { event.tagData.msg.id };

        Frame {
            bus: event.chanIndex,
            id: Identifier::Standard(tx_id),
            data,
            loopback: is_loopback(&event),
            fd: false,
        }
    }
}
