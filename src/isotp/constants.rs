use strum_macros::FromRepr;

pub static FRAME_TYPE_MASK: u8 = 0xf0;
pub static FLOW_SATUS_MASK: u8 = 0x0f;

#[derive(Debug, PartialEq, Copy, Clone, FromRepr)]
#[repr(u8)]
pub enum FrameType {
    Single = 0x00,
    First = 0x10,
    Consecutive = 0x20,
    FlowControl = 0x30,
}

#[derive(Debug, PartialEq, Copy, Clone, FromRepr)]
#[repr(u8)]
pub enum FlowStatus {
    ContinueToSend = 0x0,
    Wait = 0x1,
    Overflow = 0x2,
}
