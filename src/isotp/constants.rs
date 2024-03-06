#[derive(Debug, PartialEq, Copy, Clone)]
#[repr(u8)]
pub enum FrameType {
    Single = 0x00,
    First = 0x10,
    Consecutive = 0x20,
    FlowControl = 0x30,
    Unknown = 0xff,
}

pub static FRAME_TYPE_MASK: u8 = 0xf0;

impl From<u8> for FrameType {
    fn from(val: u8) -> FrameType {
        match val {
            0x00 => FrameType::Single,
            0x10 => FrameType::First,
            0x20 => FrameType::Consecutive,
            0x30 => FrameType::FlowControl,
            _ => FrameType::Unknown,
        }
    }
}
