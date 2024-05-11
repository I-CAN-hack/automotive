use bitflags::bitflags;
use libc::{
    can_frame, canfd_frame, canid_t, CANFD_BRS, CANFD_ESI, CAN_EFF_FLAG, CAN_ERR_FLAG, CAN_RTR_FLAG,
};

use crate::can::{Frame, Identifier};

#[inline(always)]
pub fn can_frame_default() -> can_frame {
    unsafe { std::mem::zeroed() }
}

#[inline(always)]
pub fn canfd_frame_default() -> canfd_frame {
    unsafe { std::mem::zeroed() }
}

bitflags! {
    /// Bit flags in the composite SocketCAN ID word.
    pub struct IdFlags: canid_t {
        /// Indicates frame uses a 29-bit extended ID
        const EFF = CAN_EFF_FLAG;
        /// Indicates a remote request frame.
        const RTR = CAN_RTR_FLAG;
        /// Indicates an error frame.
        const ERR = CAN_ERR_FLAG;
    }

    /// Bit flags for the Flexible Data (FD) frames.
    pub struct FdFlags: u8 {
        /// Bit rate switch (second bit rate for payload data)
        const BRS = CANFD_BRS as u8;
        /// Error state indicator of the transmitting node
        const ESI = CANFD_ESI as u8;
    }
}

fn id_to_canid_t(id: impl Into<Identifier>) -> canid_t {
    let id = id.into();
    match id {
        Identifier::Standard(id) => id as canid_t,
        Identifier::Extended(id) => id | CAN_EFF_FLAG,
    }
}

fn canid_t_to_id(id: canid_t) -> Identifier {
    match id & CAN_EFF_FLAG != 0 {
        true => Identifier::Extended(id & 0x1fffffff),
        false => Identifier::Standard(id & 0x7ff),
    }
}

impl From<can_frame> for Frame {
    fn from(frame: can_frame) -> Self {
        Self::new(
            0,
            canid_t_to_id(frame.can_id),
            &frame.data[..frame.can_dlc as usize],
        )
        .unwrap()
    }
}

impl From<canfd_frame> for Frame {
    fn from(frame: canfd_frame) -> Self {
        Self::new(
            0,
            canid_t_to_id(frame.can_id),
            &frame.data[..frame.len as usize],
        )
        .unwrap()
    }
}
