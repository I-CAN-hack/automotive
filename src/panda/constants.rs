use strum_macros::FromRepr;

#[derive(Debug, PartialEq, Copy, Clone, FromRepr)]
#[repr(u8)]
pub enum HwType {
    Unknown = 0x0,
    WhitePanda = 0x1,
    GreyPanda = 0x2,
    BlackPanda = 0x3,
    Pedal = 0x4,
    Uno = 0x5,
    Dos = 0x6,
    RedPanda = 0x7,
    RedPandaV2 = 0x8,
    Tres = 0x9,
    Quatro = 0x10,
}

#[repr(u8)]
pub enum Endpoint {
    CanWrite = 0x3,
    HwType = 0xc1,
    SafetyModel = 0xdc,
    CanSpeed = 0xde,
    CanDataSpeed = 0xf9,
    CanResetCommunications = 0xc0,
    CanRead = 0x81,
    PacketsVersions = 0xdd,
    PowerSave = 0xe7,
    CanFDAuto = 0xe8,
    HeartbeatDisabled = 0xf8,
}

#[repr(u8)]
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SafetyModel {
    Silent = 0,
    AllOutput = 17,
}

pub const FD_PANDAS: [HwType; 4] = [
    HwType::RedPanda,
    HwType::RedPandaV2,
    HwType::Tres,
    HwType::Quatro,
];
