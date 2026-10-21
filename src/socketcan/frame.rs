use libc::{can_frame, canfd_frame, canid_t, CAN_EFF_FLAG, CAN_ERR_FLAG, CAN_RTR_FLAG};

use crate::can::{Frame, FrameContent, Identifier};

// libc's flag constants have different integer types between releases.
const BRS: u8 = 0x01;
const ESI: u8 = 0x02;

pub fn can_frame_default() -> can_frame {
    unsafe { std::mem::zeroed() }
}

pub fn canfd_frame_default() -> canfd_frame {
    unsafe { std::mem::zeroed() }
}

fn id_to_canid_t(id: Identifier) -> canid_t {
    id.as_raw() | if id.is_extended() { CAN_EFF_FLAG } else { 0 }
}

fn canid_t_to_id(id: canid_t) -> crate::Result<Identifier> {
    if id & CAN_EFF_FLAG != 0 {
        Identifier::extended(id & 0x1fff_ffff)
    } else {
        Identifier::standard(id & 0x1fff_ffff)
    }
}

impl TryFrom<can_frame> for Frame {
    type Error = crate::Error;

    fn try_from(frame: can_frame) -> crate::Result<Self> {
        // SocketCAN error reports contain OS-specific diagnostics, not the
        // active/passive error flag of an ISO CAN error frame.
        if frame.can_id & CAN_ERR_FLAG != 0 {
            return Err(crate::Error::NotSupported);
        }
        if frame.can_dlc > 8 {
            return Err(crate::Error::MalformedFrame);
        }
        let id = canid_t_to_id(frame.can_id)?;
        if frame.can_id & CAN_RTR_FLAG != 0 {
            // Linux stores the byte length in can_dlc, and raw DLCs 9..15
            // separately in len8_dlc when that byte length is eight.
            let dlc = if frame.can_dlc == 8 && frame.len8_dlc > 8 {
                frame.len8_dlc
            } else {
                frame.can_dlc
            };
            return Frame::remote(0, id, dlc);
        }
        let data = frame
            .data
            .get(..frame.can_dlc as usize)
            .ok_or(crate::Error::MalformedFrame)?;
        Frame::builder(id).data(data).build()
    }
}

impl TryFrom<canfd_frame> for Frame {
    type Error = crate::Error;

    fn try_from(frame: canfd_frame) -> crate::Result<Self> {
        if frame.can_id & (CAN_RTR_FLAG | CAN_ERR_FLAG) != 0 {
            return Err(crate::Error::MalformedFrame);
        }
        let data = frame
            .data
            .get(..frame.len as usize)
            .ok_or(crate::Error::MalformedFrame)?;
        Frame::builder(canid_t_to_id(frame.can_id)?)
            .fd(true)
            .brs(frame.flags & BRS != 0)
            .esi(frame.flags & ESI != 0)
            .data(data)
            .build()
    }
}

impl TryFrom<&Frame> for can_frame {
    type Error = crate::Error;

    fn try_from(frame: &Frame) -> crate::Result<Self> {
        let id = match frame.content() {
            FrameContent::Classic { id, .. } | FrameContent::Remote { id, .. } => *id,
            FrameContent::Fd { .. } | FrameContent::Error(_) => {
                return Err(crate::Error::NotSupported)
            }
        };
        let mut raw = can_frame_default();
        raw.can_id = id_to_canid_t(id);
        if frame.is_remote() {
            raw.can_id |= CAN_RTR_FLAG;
        }
        let dlc = frame.dlc().ok_or(crate::Error::NotSupported)?;
        raw.can_dlc = dlc.min(8);
        if dlc > 8 {
            raw.len8_dlc = dlc;
        }
        raw.data[..frame.data().len()].copy_from_slice(frame.data());
        Ok(raw)
    }
}

impl TryFrom<&Frame> for canfd_frame {
    type Error = crate::Error;

    fn try_from(frame: &Frame) -> crate::Result<Self> {
        let FrameContent::Fd { id, data, brs, esi } = frame.content() else {
            return Err(crate::Error::NotSupported);
        };
        let mut raw = canfd_frame_default();
        raw.can_id = id_to_canid_t(*id);
        raw.len = data.len() as u8;
        raw.flags = if *brs { BRS } else { 0 } | if *esi { ESI } else { 0 };
        raw.data[..data.len()].copy_from_slice(data);
        Ok(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::can::{ErrorFlag, DLC_TO_LEN};

    #[test]
    fn data_round_trips_preserve_format_and_flags() {
        for id in [
            Identifier::standard(0x123).unwrap(),
            Identifier::extended(0x123).unwrap(),
        ] {
            for &len in DLC_TO_LEN {
                let data = vec![0xaa; len];
                let fd = Frame::builder(id)
                    .fd(true)
                    .brs(true)
                    .esi(true)
                    .data(&data)
                    .build()
                    .unwrap();
                let raw = canfd_frame::try_from(&fd).unwrap();
                assert_eq!(Frame::try_from(raw).unwrap(), fd);
                if len <= 8 {
                    let classic = Frame::builder(id).data(&data).build().unwrap();
                    let raw = can_frame::try_from(&classic).unwrap();
                    assert_eq!(Frame::try_from(raw).unwrap(), classic);
                    assert!(canfd_frame::try_from(&classic).is_err());
                }
                assert!(can_frame::try_from(&fd).is_err());
            }
        }
    }

    #[test]
    fn remote_round_trip_retains_requested_dlc_without_payload() {
        for dlc in 0..=15 {
            let frame = Frame::remote(0, Identifier::extended(0x123).unwrap(), dlc).unwrap();
            let raw = can_frame::try_from(&frame).unwrap();
            assert_ne!(raw.can_id & CAN_RTR_FLAG, 0);
            assert_eq!(Frame::try_from(raw).unwrap(), frame);
        }
    }

    #[test]
    fn malformed_raw_frames_return_errors() {
        let mut classic = can_frame_default();
        classic.can_dlc = 9;
        assert!(Frame::try_from(classic).is_err());
        classic.can_dlc = 0;
        classic.can_id = 0x800;
        assert!(Frame::try_from(classic).is_err());

        for len in [9, 65, u8::MAX] {
            let mut fd = canfd_frame_default();
            fd.len = len;
            assert!(Frame::try_from(fd).is_err());
        }
        let mut fd = canfd_frame_default();
        fd.can_id = CAN_RTR_FLAG;
        assert!(Frame::try_from(fd).is_err());
    }

    #[test]
    fn error_frames_are_not_sent_or_misread_as_data() {
        let error = Frame::error(0, ErrorFlag::Active);
        assert!(can_frame::try_from(&error).is_err());
        assert!(canfd_frame::try_from(&error).is_err());
        let mut raw = can_frame_default();
        raw.can_id = CAN_ERR_FLAG;
        assert!(matches!(
            Frame::try_from(raw),
            Err(crate::Error::NotSupported)
        ));
    }
}
