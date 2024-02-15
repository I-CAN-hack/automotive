#[derive(Debug, PartialEq, Copy, Clone)]
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

impl From<u8> for HwType {
    fn from(val: u8) -> HwType {
        match val {
            0x0 => HwType::Unknown,
            0x1 => HwType::WhitePanda,
            0x2 => HwType::GreyPanda,
            0x3 => HwType::BlackPanda,
            0x4 => HwType::Pedal,
            0x5 => HwType::Uno,
            0x6 => HwType::Dos,
            0x7 => HwType::RedPanda,
            0x8 => HwType::RedPandaV2,
            0x9 => HwType::Tres,
            0x10 => HwType::Quatro,
            _ => panic!("Invalid HwType value"),
        }
    }
}
