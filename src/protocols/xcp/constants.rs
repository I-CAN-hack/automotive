//! XCP wire codes. Unknown values round-trip through `NonStandard` and `as_raw`.

wire_code_enum! {
    /// XCP command codes, including the prefix for level-one commands.
    pub enum Command: u8 {
        Connect = 0xff,
        Disconnect = 0xfe,
        GetStatus = 0xfd,
        Synch = 0xfc,
        GetCommModeInfo = 0xfb,
        GetId = 0xfa,
        SetRequest = 0xf9,
        GetSeed = 0xf8,
        Unlock = 0xf7,
        SetMta = 0xf6,
        Upload = 0xf5,
        ShortUpload = 0xf4,
        BuildChecksum = 0xf3,
        TransportLayerCmd = 0xf2,
        UserCmd = 0xf1,
        Download = 0xf0,
        DownloadNext = 0xef,
        DownloadMax = 0xee,
        ShortDownload = 0xed,
        ModifyBits = 0xec,
        SetCalPage = 0xeb,
        GetCalPage = 0xea,
        GetPagProcessorInfo = 0xe9,
        GetSegmentInfo = 0xe8,
        GetPageInfo = 0xe7,
        SetSegmentMode = 0xe6,
        GetSegmentMode = 0xe5,
        CopyCalPage = 0xe4,
        ClearDaqList = 0xe3,
        SetDaqPtr = 0xe2,
        WriteDaq = 0xe1,
        SetDaqListMode = 0xe0,
        GetDaqListMode = 0xdf,
        StartStopDaqList = 0xde,
        StartStopSynch = 0xdd,
        GetDaqClock = 0xdc,
        ReadDaq = 0xdb,
        GetDaqProcessorInfo = 0xda,
        GetDaqResolutionInfo = 0xd9,
        GetDaqListInfo = 0xd8,
        GetDaqEventInfo = 0xd7,
        FreeDaq = 0xd6,
        AllocDaq = 0xd5,
        AllocOdt = 0xd4,
        AllocOdtEntry = 0xd3,
        ProgramStart = 0xd2,
        ProgramClear = 0xd1,
        Program = 0xd0,
        ProgramReset = 0xcf,
        GetPgmProcessorInfo = 0xce,
        GetSectorInfo = 0xcd,
        ProgramPrepare = 0xcc,
        ProgramFormat = 0xcb,
        ProgramNext = 0xca,
        ProgramMax = 0xc9,
        ProgramVerify = 0xc8,
        WriteDaqMultiple = 0xc7,
        TimeCorrelationProperties = 0xc6,
        DtoCtrProperties = 0xc5,
        Level1 = 0xc0,
    }
}

wire_code_enum! {
    /// Single-resource bits. CONNECT/GET_STATUS masks may contain combinations.
    pub enum Resource: u8 {
        CalPag = 0x01,
        Daq = 0x04,
        Stim = 0x08,
        Pgm = 0x10,
        Dbg = 0x20,
    }
}

wire_code_enum! {
    /// CONNECT mode parameter.
    pub enum ConnectMode: u8 {
        Normal = 0x00,
        UserDefined = 0x01,
    }
}

wire_code_enum! {
    /// GET_ID identification request type.
    pub enum IdentificationType: u8 {
        Ascii = 0x00,
        FileName = 0x01,
        Path = 0x02,
        Url = 0x03,
        Upload = 0x04,
    }
}
