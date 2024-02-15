#[repr(u8)]
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SafetyModel {
    Silent = 0,
    AllOutput = 17,
}
