use strum_macros::FromRepr;

use crate::vector::bindings as xl;
pub use crate::vector::bindings::{
    XLaccess, XLcanFdConf, XLcanRxEvent, XLcanTxEvent, XLportHandle,
};

pub static DLC_TO_LEN: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];
pub static LEN_TO_DLC: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 9, 0, 0, 0, 10, 0, 0, 0, 11, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0,
    0, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 15,
];

pub const XL_CAN_EV_TAG_TX_MSG: u16 = 0x440;

#[repr(u16)]
#[allow(non_camel_case_types)]
#[derive(Debug, FromRepr, PartialEq)]
pub enum RxTags {
    XL_CAN_EV_TAG_RX_OK = 0x0400,
    XL_CAN_EV_TAG_RX_ERROR = 0x0401,
    XL_CAN_EV_TAG_TX_ERROR = 0x0402,
    XL_CAN_EV_TAG_TX_REQUEST = 0x0403,
    XL_CAN_EV_TAG_TX_OK = 0x0404,
    XL_CAN_EV_TAG_CHIP_STATE = 0x0409,
}

#[repr(u32)]
#[derive(FromRepr, Debug, PartialEq, Copy, Clone)]
pub enum HwType {
    None = xl::XL_HWTYPE_NONE,
    Virtual = xl::XL_HWTYPE_VIRTUAL,
    CANcardX = xl::XL_HWTYPE_CANCARDX,
    CANAC2PCI = xl::XL_HWTYPE_CANAC2PCI,
    CANcardY = xl::XL_HWTYPE_CANCARDY,
    CANcardXL = xl::XL_HWTYPE_CANCARDXL,
    CANcaseXL = xl::XL_HWTYPE_CANCASEXL,
    CANcaseXLLogObsolete = xl::XL_HWTYPE_CANCASEXL_LOG_OBSOLETE,
    CANboardXL = xl::XL_HWTYPE_CANBOARDXL,
    CANboardXLPXI = xl::XL_HWTYPE_CANBOARDXL_PXI,
    VN2600 = xl::XL_HWTYPE_VN2600,
    // VN2610 = xl::XL_HWTYPE_VN2610,
    VN3300 = xl::XL_HWTYPE_VN3300,
    VN3600 = xl::XL_HWTYPE_VN3600,
    VN7600 = xl::XL_HWTYPE_VN7600,
    VNCANcardXLE = xl::XL_HWTYPE_CANCARDXLE,
    VN8900 = xl::XL_HWTYPE_VN8900,
    VN8950 = xl::XL_HWTYPE_VN8950,
    VN2640 = xl::XL_HWTYPE_VN2640,
    VN1610 = xl::XL_HWTYPE_VN1610,
    VN1630 = xl::XL_HWTYPE_VN1630,
    VN1640 = xl::XL_HWTYPE_VN1640,
    VN8970 = xl::XL_HWTYPE_VN8970,
    VN1611 = xl::XL_HWTYPE_VN1611,
    VN5240 = xl::XL_HWTYPE_VN5240,
    VN5610 = xl::XL_HWTYPE_VN5610,
    VN5620 = xl::XL_HWTYPE_VN5620,
    VN7570 = xl::XL_HWTYPE_VN7570,
    VN5650 = xl::XL_HWTYPE_VN5650,
    IPCCLient = xl::XL_HWTYPE_IPCLIENT,
    IPServer = xl::XL_HWTYPE_IPSERVER,
    VX1121 = xl::XL_HWTYPE_VX1121,
    VX1131 = xl::XL_HWTYPE_VX1131,
    VT6204 = xl::XL_HWTYPE_VT6204,
    VN1630Log = xl::XL_HWTYPE_VN1630_LOG,
    VN7610 = xl::XL_HWTYPE_VN7610,
    VN7572 = xl::XL_HWTYPE_VN7572,
    VN8972 = xl::XL_HWTYPE_VN8972,
    VN0601 = xl::XL_HWTYPE_VN0601,
    VN5640 = xl::XL_HWTYPE_VN5640,
    VX0312 = xl::XL_HWTYPE_VX0312,
    VH6501 = xl::XL_HWTYPE_VH6501,
    VN8800 = xl::XL_HWTYPE_VN8800,
    IPCL8800 = xl::XL_HWTYPE_IPCL8800,
    IPSRV8800 = xl::XL_HWTYPE_IPSRV8800,
    CSMCAN = xl::XL_HWTYPE_CSMCAN,
    VN5610A = xl::XL_HWTYPE_VN5610A,
    VN7640 = xl::XL_HWTYPE_VN7640,
    VX1135 = xl::XL_HWTYPE_VX1135,
    VN4610 = xl::XL_HWTYPE_VN4610,
    VT6306 = xl::XL_HWTYPE_VT6306,
    VT6104A = xl::XL_HWTYPE_VT6104A,
    VN5430 = xl::XL_HWTYPE_VN5430,
    VTService = xl::XL_HWTYPE_VTSSERVICE,
    VN1530 = xl::XL_HWTYPE_VN1530,
    VN1531 = xl::XL_HWTYPE_VN1531,
    VX1161A = xl::XL_HWTYPE_VX1161A,
    VX1161B = xl::XL_HWTYPE_VX1161B,
}

#[derive(Debug, Copy, Clone)]
pub struct ChannelConfig {
    pub hw_type: HwType,
    pub hw_index: u32,
    pub hw_channel: u32,
}

#[derive(Debug, Copy, Clone)]
pub struct PortHandle {
    pub port_handle: XLportHandle,
    pub permission_mask: XLaccess,
}

impl From<crate::can::Frame> for XLcanTxEvent {
    fn from(frame: crate::can::Frame) -> Self {
        let can_id = match frame.id {
            crate::can::Id::Standard(id) => id.as_raw().into(),
            crate::can::Id::Extended(id) => id.as_raw() | xl::XL_CAN_EXT_MSG_ID,
        };
        let flags = match frame.fd {
            true => xl::XL_CAN_TXMSG_FLAG_EDL,
            false => 0,
        };

        // TODO: move calculation to can::Frame?
        let dlc = LEN_TO_DLC[frame.data.len()];

        // Copy data into array
        let mut data = [0; xl::XL_CAN_MAX_DATA_LEN as usize];
        data[..frame.data.len()].copy_from_slice(&frame.data);

        Self {
            tag: XL_CAN_EV_TAG_TX_MSG,
            transId: 0,      // Internal use
            channelIndex: 0, // Internal use. The accessMask parameter of xlCanTransmitEx() specifies which channels send the message
            reserved: [0, 0, 0],
            tagData: xl::XLcanTxEvent__bindgen_ty_1 {
                canMsg: xl::XL_CAN_TX_MSG {
                    canId: can_id,
                    msgFlags: flags,
                    dlc,
                    reserved: [0, 0, 0, 0, 0, 0, 0],
                    data,
                },
            },
        }
    }
}

impl TryFrom<XLcanRxEvent> for crate::can::Frame {
    type Error = ();

    fn try_from(event: XLcanRxEvent) -> Result<Self, Self::Error> {
        let tag = RxTags::from_repr(event.tag).unwrap();

        match tag {
            RxTags::XL_CAN_EV_TAG_TX_OK | RxTags::XL_CAN_EV_TAG_RX_OK => {
                let frame = unsafe { event.tagData.canRxOkMsg }; // Same type for both rx and tx
                let loopback = tag == RxTags::XL_CAN_EV_TAG_TX_OK;

                let id = match frame.canId & xl::XL_CAN_EXT_MSG_ID != 0 {
                    false => crate::can::StandardId::new(frame.canId as u16 & 0x7ff)
                        .ok_or(())?
                        .into(),
                    true => crate::can::ExtendedId::new(frame.canId & 0x1fffffff)
                        .ok_or(())?
                        .into(),
                };
                let len = DLC_TO_LEN[frame.dlc as usize];
                let fd = frame.msgFlags & xl::XL_CAN_RXMSG_FLAG_EDL != 0;

                Ok(Self {
                    bus: 0, // TODO: perform proper mapping based on xlGetChannelIndex,
                    id,
                    data: frame.data[..len].into(),
                    loopback,
                    fd,
                })
            }
            RxTags::XL_CAN_EV_TAG_CHIP_STATE | RxTags::XL_CAN_EV_TAG_TX_ERROR => {
                Err(()) // Ignore these for now
            }
            _ => {
                tracing::warn!("xlCanReceive unhandled tag {:?}", tag);
                Err(())
            }
        }
    }
}
