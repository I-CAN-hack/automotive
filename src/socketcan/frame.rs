use embedded_can::{ExtendedId, Id, StandardId};
use libc::{can_frame, canfd_frame, canid_t, CANFD_MAX_DLEN, CAN_EFF_FLAG, CAN_MAX_DLC};

use crate::can::Frame;

pub fn can_frame_default() -> can_frame {
    unsafe { std::mem::zeroed() }
}

pub fn canfd_frame_default() -> canfd_frame {
    unsafe { std::mem::zeroed() }
}

fn id_to_canid_t(id: Id) -> canid_t {
    match id {
        Id::Standard(id) => id.as_raw().into(),
        Id::Extended(id) => id.as_raw() | CAN_EFF_FLAG,
    }
}

fn canid_t_to_id(id: canid_t) -> Id {
    // These unwraps are safe because the IDs are guaranteed to be valid
    match id & CAN_EFF_FLAG != 0 {
        true => ExtendedId::new(id & 0x1fffffff).unwrap().into(),
        false => StandardId::new(id as u16 & 0x7ff).unwrap().into(),
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

impl From<&Frame> for canfd_frame {
    fn from(frame: &Frame) -> canfd_frame {
        assert!(frame.fd);
        assert!(frame.data.len() <= CANFD_MAX_DLEN);

        let mut raw_frame = canfd_frame_default();
        raw_frame.can_id = id_to_canid_t(frame.id);
        raw_frame.len = frame.data.len() as u8;
        // TODO: Set flags like BRS
        raw_frame.data[..frame.data.len()].copy_from_slice(&frame.data);

        raw_frame
    }
}
