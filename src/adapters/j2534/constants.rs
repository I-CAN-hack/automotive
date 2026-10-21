//! J2534 04.04 protocol constants expressed as type-safe enums.

/// `TxFlags` flag: pad outbound ISO 15765 CAN frames to DLC = 8.
pub const ISO15765_FRAME_PAD: u32 = 0x0040;
/// `TxFlags` / `Connect` flag: ISO 15765 extended addressing.
pub const ISO15765_ADDR_TYPE: u32 = 0x0080;
/// `RxStatus` / `TxFlags` flag: message uses a 29-bit CAN ID.
pub const CAN_29BIT_ID_FLAG: u32 = 0x0100;
/// `Connect` flag: channel accepts both 11-bit and 29-bit CAN identifiers.
pub const CAN_ID_BOTH: u32 = 0x0800;
/// `RxStatus` flag: received ISO 15765 frame had a padding error.
pub const ISO15765_PADDING_ERROR: u32 = 0x0010;

/// J2534 status / error codes returned by all PassThru functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Status {
    NoError = 0x00,
    NotSupported = 0x01,
    InvalidChannelId = 0x02,
    InvalidProtocolId = 0x03,
    NullParameter = 0x04,
    InvalidIoctlValue = 0x05,
    InvalidFlags = 0x06,
    Failed = 0x07,
    DeviceNotConnected = 0x08,
    Timeout = 0x09,
    InvalidMsg = 0x0A,
    InvalidTimeInterval = 0x0B,
    ExceededLimit = 0x0C,
    InvalidMsgId = 0x0D,
    DeviceInUse = 0x0E,
    InvalidIoctlId = 0x0F,
    BufferEmpty = 0x10,
    BufferFull = 0x11,
    BufferOverflow = 0x12,
    PinInvalid = 0x13,
    ChannelInUse = 0x14,
    MsgProtocolId = 0x15,
    InvalidFilterId = 0x16,
    NoFlowControl = 0x17,
    NotUnique = 0x18,
    InvalidBaudrate = 0x19,
    InvalidDeviceId = 0x1A,
}

impl From<i32> for Status {
    fn from(code: i32) -> Self {
        match code {
            0x00 => Self::NoError,
            0x01 => Self::NotSupported,
            0x02 => Self::InvalidChannelId,
            0x03 => Self::InvalidProtocolId,
            0x04 => Self::NullParameter,
            0x05 => Self::InvalidIoctlValue,
            0x06 => Self::InvalidFlags,
            0x07 => Self::Failed,
            0x08 => Self::DeviceNotConnected,
            0x09 => Self::Timeout,
            0x0A => Self::InvalidMsg,
            0x0B => Self::InvalidTimeInterval,
            0x0C => Self::ExceededLimit,
            0x0D => Self::InvalidMsgId,
            0x0E => Self::DeviceInUse,
            0x0F => Self::InvalidIoctlId,
            0x10 => Self::BufferEmpty,
            0x11 => Self::BufferFull,
            0x12 => Self::BufferOverflow,
            0x13 => Self::PinInvalid,
            0x14 => Self::ChannelInUse,
            0x15 => Self::MsgProtocolId,
            0x16 => Self::InvalidFilterId,
            0x17 => Self::NoFlowControl,
            0x18 => Self::NotUnique,
            0x19 => Self::InvalidBaudrate,
            0x1A => Self::InvalidDeviceId,
            _ => Self::Failed,
        }
    }
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoError => "STATUS_NOERROR",
            Self::NotSupported => "ERR_NOT_SUPPORTED",
            Self::InvalidChannelId => "ERR_INVALID_CHANNEL_ID",
            Self::InvalidProtocolId => "ERR_INVALID_PROTOCOL_ID",
            Self::NullParameter => "ERR_NULL_PARAMETER",
            Self::InvalidIoctlValue => "ERR_INVALID_IOCTL_VALUE",
            Self::InvalidFlags => "ERR_INVALID_FLAGS",
            Self::Failed => "ERR_FAILED",
            Self::DeviceNotConnected => "ERR_DEVICE_NOT_CONNECTED",
            Self::Timeout => "ERR_TIMEOUT",
            Self::InvalidMsg => "ERR_INVALID_MSG",
            Self::InvalidTimeInterval => "ERR_INVALID_TIME_INTERVAL",
            Self::ExceededLimit => "ERR_EXCEEDED_LIMIT",
            Self::InvalidMsgId => "ERR_INVALID_MSG_ID",
            Self::DeviceInUse => "ERR_DEVICE_IN_USE",
            Self::InvalidIoctlId => "ERR_INVALID_IOCTL_ID",
            Self::BufferEmpty => "ERR_BUFFER_EMPTY",
            Self::BufferFull => "ERR_BUFFER_FULL",
            Self::BufferOverflow => "ERR_BUFFER_OVERFLOW",
            Self::PinInvalid => "ERR_PIN_INVALID",
            Self::ChannelInUse => "ERR_CHANNEL_IN_USE",
            Self::MsgProtocolId => "ERR_MSG_PROTOCOL_ID",
            Self::InvalidFilterId => "ERR_INVALID_FILTER_ID",
            Self::NoFlowControl => "ERR_NO_FLOW_CONTROL",
            Self::NotUnique => "ERR_NOT_UNIQUE",
            Self::InvalidBaudrate => "ERR_INVALID_BAUDRATE",
            Self::InvalidDeviceId => "ERR_INVALID_DEVICE_ID",
        }
    }
}

impl From<Status> for i32 {
    fn from(s: Status) -> i32 {
        s as i32
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// SAE J2534 protocol identifiers passed to `PassThruConnect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Protocol {
    Can = 5,
    Iso15765 = 6,
}

impl From<Protocol> for u32 {
    fn from(p: Protocol) -> u32 {
        p as u32
    }
}

/// `PassThruStartMsgFilter` filter types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FilterType {
    Pass = 1,
    FlowControl = 3,
}

impl From<FilterType> for u32 {
    fn from(f: FilterType) -> u32 {
        f as u32
    }
}

/// `PassThruIoctl` command identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum IoctlId {
    SetConfig = 0x02,
    ClearRxBuffer = 0x08,
}

impl From<IoctlId> for u32 {
    fn from(id: IoctlId) -> u32 {
        id as u32
    }
}

/// Channel parameters used with `SET_CONFIG` ioctl.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum IoctlParam {
    Iso15765Stmin = 0x1F,
    StminTx = 0x23,
}

impl From<IoctlParam> for u32 {
    fn from(p: IoctlParam) -> u32 {
        p as u32
    }
}
