wire_code_enum! {
    /// CCP 2.1 memory transfer and session commands.
    pub enum Command: u8 {
        Connect = 0x01,
        SetMta = 0x02,
        Download = 0x03,
        Upload = 0x04,
        Disconnect = 0x07,
        ShortUpload = 0x0f,
        GetSeed = 0x12,
        Unlock = 0x13,
        ExchangeId = 0x17,
        GetCcpVersion = 0x1b,
        Download6 = 0x23,
    }
}

wire_code_enum! {
    /// CCP resource masks.
    pub enum Resource: u8 {
        Cal = 0x01,
        Daq = 0x02,
        Pgm = 0x40,
    }
}
