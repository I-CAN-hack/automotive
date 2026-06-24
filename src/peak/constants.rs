//! Protocol constants for the PEAK PCAN-USB FD family adapters.
//!
//! These adapters speak the "uCAN" message/command protocol over raw USB bulk
//! endpoints. The values below describe that wire format. Some constants are
//! kept for completeness as protocol documentation even if currently unused.
#![allow(dead_code)]

/// PEAK System USB vendor ID.
pub const USB_VID: u16 = 0x0c72;

/// Product IDs of the PCAN-USB FD family. All of these speak the uCAN protocol
/// implemented by this module. The classic PCAN-USB / PCAN-USB Pro (non-FD) use
/// a different, older protocol and are intentionally not supported here.
pub const PID_PCAN_USB_PRO_FD: u16 = 0x0011;
pub const PID_PCAN_USB_FD: u16 = 0x0012;
pub const PID_PCAN_USB_CHIP: u16 = 0x0013;
pub const PID_PCAN_USB_X6: u16 = 0x0014;

/// All supported product IDs.
pub const SUPPORTED_PIDS: &[u16] = &[
    PID_PCAN_USB_PRO_FD,
    PID_PCAN_USB_FD,
    PID_PCAN_USB_CHIP,
    PID_PCAN_USB_X6,
];

/// CAN controller input clock in Hz.
pub const CLOCK_HZ: u32 = 80_000_000;

/// Default USB endpoints, used unless the firmware advertises its own.
/// Command endpoints carry the uCAN command list, message endpoints carry frames.
pub const DEFAULT_EP_CMD_OUT: u8 = 0x01;
pub const DEFAULT_EP_CMD_IN: u8 = 0x81;
pub const DEFAULT_EP_MSG_OUT: u8 = 0x02;
pub const DEFAULT_EP_MSG_IN: u8 = 0x82;

/// USB control transfer: vendor request, recipient "other".
/// Combined with the IN/OUT direction bit by the control helpers.
pub const CTRL_REQ_INFO: u8 = 0;
pub const CTRL_REQ_FCT: u8 = 2;

/// `wValue` for the firmware-info request.
pub const INFO_FW: u16 = 1;

/// `wValue` and payload length to tell the device the driver is (un)loaded.
pub const FCT_DRV_LOADED: u16 = 5;
pub const FCT_DRV_LOADED_LEN: usize = 16;

/// Length of the firmware-info structure returned by [`CTRL_REQ_INFO`]/[`INFO_FW`].
pub const FW_INFO_LEN: usize = 36;
/// Offsets into the firmware-info structure.
pub const FW_INFO_TYPE_OFFSET: usize = 2;
pub const FW_INFO_FW_VERSION_OFFSET: usize = 9;
pub const FW_INFO_CMD_OUT_EP_OFFSET: usize = 28;
pub const FW_INFO_CMD_IN_EP_OFFSET: usize = 29;
pub const FW_INFO_DATA_OUT_EP_OFFSET: usize = 30;
pub const FW_INFO_DATA_IN_EP_OFFSET: usize = 32;
/// Firmware-info `type` value at/above which endpoint numbers are embedded.
pub const FW_INFO_TYPE_EXT: u16 = 2;

/// uCAN command opcodes (low 10 bits of the `opcode_channel` field).
pub const CMD_RESET_MODE: u16 = 0x001;
pub const CMD_NORMAL_MODE: u16 = 0x002;
pub const CMD_LISTEN_ONLY_MODE: u16 = 0x003;
pub const CMD_TIMING_SLOW: u16 = 0x004;
pub const CMD_TIMING_FAST: u16 = 0x005;
pub const CMD_FILTER_STD: u16 = 0x008;
pub const CMD_WR_ERR_CNT: u16 = 0x00a;
pub const CMD_SET_EN_OPTION: u16 = 0x00b;
pub const CMD_CLR_DIS_OPTION: u16 = 0x00c;
pub const CMD_END_OF_COLLECTION: u16 = 0x3ff;

/// Extended (vendor-specific, non-uCAN) commands.
pub const CMD_CLOCK_SET: u16 = 0x80;
pub const CMD_LED_SET: u16 = 0x86;

/// Clock mode selecting the 80 MHz domain.
pub const CLOCK_80MHZ: u8 = 0x00;

/// Default error-warning limit used in the slow bit-timing command.
pub const DEFAULT_ERROR_WARNING_LIMIT: u8 = 96;

/// `WR_ERR_CNT` select-mask bits (enable writing the Tx / Rx counters).
pub const WRERRCNT_TX_ENABLE: u16 = 0x4000;
pub const WRERRCNT_RX_ENABLE: u16 = 0x8000;

/// Option bit selecting ISO CAN-FD framing.
pub const OPTION_CAN_FD_ISO: u16 = 0x0004;

/// uCAN message types (the `type` field of a record).
pub const MSG_CAN_RX: u16 = 0x0001;
pub const MSG_ERROR: u16 = 0x0002;
pub const MSG_STATUS: u16 = 0x0003;
pub const MSG_CALIBRATION: u16 = 0x0100;
pub const MSG_OVERRUN: u16 = 0x0101;
pub const MSG_CAN_TX: u16 = 0x1000;

/// uCAN CAN-message flag bits (the 16-bit `flags` field).
pub const FLAG_SELF_RECEIVE: u16 = 0x80;
pub const FLAG_ERROR_STATE_IND: u16 = 0x40;
pub const FLAG_BITRATE_SWITCH: u16 = 0x20;
pub const FLAG_EXT_DATA_LEN: u16 = 0x10;
pub const FLAG_SINGLE_SHOT: u16 = 0x08;
pub const FLAG_LOOPED_BACK: u16 = 0x04;
pub const FLAG_EXT_ID: u16 = 0x02;
pub const FLAG_RTR: u16 = 0x01;

/// Header size (bytes) of a transmitted CAN message record.
pub const TX_MSG_HEADER_SIZE: usize = 20;
/// Header size (bytes) of a received CAN message record.
pub const RX_MSG_HEADER_SIZE: usize = 28;
/// Size (bytes) of a single command record. All commands share this size.
pub const COMMAND_SIZE: usize = 8;

/// Size of the device command buffer. A single USB transfer of commands must
/// not exceed this.
pub const CMD_BUFFER_SIZE: usize = 512;
/// Size of a single USB transfer of transmitted CAN messages.
pub const TX_BUFFER_SIZE: usize = 512;
/// Size of the buffer used to read received CAN messages.
pub const RX_BUFFER_SIZE: usize = 2048;
