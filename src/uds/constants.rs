#[derive(Debug, PartialEq, Copy, Clone)]
#[repr(u8)]
pub enum ServiceIdentifier {
    TesterPresent = 0x3e,
    NegativeResponse = 0x7f,
}
