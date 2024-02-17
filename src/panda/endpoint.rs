#[repr(u8)]
pub enum Endpoint {
    CanWrite = 0x3,
    HwType = 0xc1,
    SafetyModel = 0xdc,
    CanResetCommunications = 0xc0,
    CanRead = 0x81,
    PacketsVersions = 0xdd,
    PowerSave = 0xe7,
    HeartbeatDisabled = 0xf8,
}
