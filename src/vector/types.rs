use strum_macros::FromRepr;

use crate::can::bitrate::BitrateConfig;
use crate::vector::bindings as xl;
pub use crate::vector::bindings::{
    XLaccess, XLcanFdConf, XLcanRxEvent, XLcanTxEvent, XLportHandle,
};

use crate::can::{FrameContent, DLC_TO_LEN};

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

impl PartialEq for XLcanFdConf {
    fn eq(&self, other: &Self) -> bool {
        self.arbitrationBitRate == other.arbitrationBitRate
            && self.sjwAbr == other.sjwAbr
            && self.tseg1Abr == other.tseg1Abr
            && self.tseg2Abr == other.tseg2Abr
            && self.dataBitRate == other.dataBitRate
            && self.sjwDbr == other.sjwDbr
            && self.tseg1Dbr == other.tseg1Dbr
            && self.tseg2Dbr == other.tseg2Dbr
            && self.reserved == other.reserved
            && self.options == other.options
            && self.reserved1 == other.reserved1
            && self.reserved2 == other.reserved2
    }
}

impl Eq for XLcanFdConf {}

impl From<BitrateConfig> for XLcanFdConf {
    fn from(config: BitrateConfig) -> Self {
        let nominal = config.nominal;
        let data = match config.data {
            Some(data) => data,
            _ => {
                tracing::warn!("no data bitrate settings provided when building XLcanFdConf");
                nominal
            }
        };

        Self {
            arbitrationBitRate: nominal.bitrate,
            sjwAbr: nominal.sjw,
            tseg1Abr: nominal.tseg1,
            tseg2Abr: nominal.tseg2,
            dataBitRate: data.bitrate,
            sjwDbr: data.sjw,
            tseg1Dbr: data.tseg1,
            tseg2Dbr: data.tseg2,
            reserved: 0,
            options: 0,
            reserved1: [0, 0],
            reserved2: 0,
        }
    }
}

impl TryFrom<&crate::can::Frame> for XLcanTxEvent {
    type Error = crate::Error;

    fn try_from(frame: &crate::can::Frame) -> crate::Result<Self> {
        let id = frame.id().ok_or(crate::Error::NotSupported)?;
        let can_id = id.as_raw()
            | if id.is_extended() {
                xl::XL_CAN_EXT_MSG_ID
            } else {
                0
            };
        let mut flags = 0;
        match frame.content() {
            FrameContent::Fd { brs, esi, .. } => {
                if *esi {
                    return Err(crate::Error::NotSupported);
                }
                flags |= xl::XL_CAN_TXMSG_FLAG_EDL;
                if *brs {
                    flags |= xl::XL_CAN_TXMSG_FLAG_BRS;
                }
            }
            FrameContent::Remote { .. } => flags |= xl::XL_CAN_TXMSG_FLAG_RTR,
            FrameContent::Classic { .. } => {}
            FrameContent::Error(_) => return Err(crate::Error::NotSupported),
        }
        let dlc = frame.dlc().ok_or(crate::Error::NotSupported)?;
        let mut data = [0; xl::XL_CAN_MAX_DATA_LEN as usize];
        data[..frame.data().len()].copy_from_slice(frame.data());

        Ok(Self {
            tag: XL_CAN_EV_TAG_TX_MSG,
            transId: 0,
            channelIndex: 0, // The transmit accessMask specifies the channel.
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
        })
    }
}

impl TryFrom<XLcanRxEvent> for crate::can::Frame {
    type Error = ();

    fn try_from(event: XLcanRxEvent) -> Result<Self, Self::Error> {
        let tag = RxTags::from_repr(event.tag).ok_or(())?;

        match tag {
            RxTags::XL_CAN_EV_TAG_TX_OK | RxTags::XL_CAN_EV_TAG_RX_OK => {
                let frame = unsafe { event.tagData.canRxOkMsg }; // Same type for both rx and tx
                let loopback = tag == RxTags::XL_CAN_EV_TAG_TX_OK;

                let id = if frame.canId & xl::XL_CAN_EXT_MSG_ID != 0 {
                    crate::can::Identifier::extended(frame.canId & 0x1fff_ffff)
                } else {
                    crate::can::Identifier::standard(frame.canId & 0x7ff)
                }
                .map_err(|_| ())?;
                let fd = frame.msgFlags & xl::XL_CAN_RXMSG_FLAG_EDL != 0;
                let rtr = frame.msgFlags & xl::XL_CAN_RXMSG_FLAG_RTR != 0;
                let len = *DLC_TO_LEN.get(frame.dlc as usize).ok_or(())?;
                if rtr {
                    if fd {
                        return Err(());
                    }
                    let mut remote = Self::remote(0, id, frame.dlc).map_err(|_| ())?;
                    remote.loopback = loopback;
                    return Ok(remote);
                }
                let len = if fd { len } else { len.min(8) };
                Self::builder(id)
                    .loopback(loopback)
                    .data(&frame.data[..len])
                    .fd(fd)
                    .brs(fd && frame.msgFlags & xl::XL_CAN_RXMSG_FLAG_BRS != 0)
                    .esi(fd && frame.msgFlags & xl::XL_CAN_RXMSG_FLAG_ESI != 0)
                    .build()
                    .map_err(|_| ())
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
