use libc::{can_frame, canfd_frame, canid_t, CANFD_MAX_DLEN, CAN_EFF_FLAG, CAN_MAX_DLC};

use crate::can::{Frame, Identifier};

pub fn can_frame_default() -> can_frame {
    unsafe { std::mem::zeroed() }
}

pub fn canfd_frame_default() -> canfd_frame {
    unsafe { std::mem::zeroed() }
}

fn id_to_canid_t(id: Identifier) -> canid_t {
    match id {
        Identifier::Standard(id) => id,
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

impl From<&Frame> for can_frame {
    fn from(frame: &Frame) -> can_frame {
        assert!(!frame.fd);
        assert!(frame.data.len() <= CAN_MAX_DLC as usize);

        let mut raw_frame = can_frame_default();
        raw_frame.can_id = id_to_canid_t(frame.id);
        raw_frame.can_dlc = frame.data.len() as u8;
        raw_frame.data[..frame.data.len()].copy_from_slice(&frame.data);

        raw_frame
    }
}

impl From<socketcan::Id> for crate::can::Identifier {
    fn from(id: socketcan::Id) -> Self {
        match id {
            socketcan::Id::Standard(id) => crate::can::Identifier::Standard(id.as_raw() as u32),
            socketcan::Id::Extended(id) => crate::can::Identifier::Extended(id.as_raw()),
        }
    }
}

impl From<crate::can::Identifier> for socketcan::Id {
    fn from(id: crate::can::Identifier) -> Self {
        match id {
            crate::can::Identifier::Standard(id) => {
                socketcan::Id::Standard(socketcan::StandardId::new(id as u16).unwrap())
            }
            crate::can::Identifier::Extended(id) => {
                socketcan::Id::Extended(socketcan::ExtendedId::new(id).unwrap())
            }
        }
    }
}
