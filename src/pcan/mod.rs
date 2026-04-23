//! PEAK PCAN adapter support (macOS via the MacCAN PCBUSB library).
//!
//! This adapter talks to PEAK's USB CAN interfaces (PCAN-USB and PCAN-USB FD)
//! through UV Software's [PCBUSB library][pcbusb] — a macOS port of PEAK's
//! PCAN-Basic API. PCBUSB must be installed on the host; pre-built universal
//! binaries ship the dylib as `/usr/local/lib/libPCBUSB.dylib`.
//!
//! Both classic CAN and CAN-FD are supported:
//! - If the [`BitrateConfig`] has no data phase, the adapter opens the channel
//!   with `CAN_Initialize` and a validated BTR0BTR1 constant derived from the
//!   nominal bitrate.
//! - If the config includes a data phase, the adapter opens the channel with
//!   `CAN_InitializeFD` and a bitrate string built from the resolved timing.
//!
//! PCBUSB does not expose a reliable hardware loopback stream, so outgoing
//! frames are synthesised back into the receive queue on successful transmit,
//! mirroring the emulated-ACK path the SocketCAN adapter uses when `IFF_ECHO`
//! is unavailable.
//!
//! [pcbusb]: https://github.com/mac-can/PCBUSB-Library

mod error;
mod sys;

pub use error::{Error, PcanStatus};

use std::collections::VecDeque;
use std::ffi::{c_void, CString};

use tracing::{debug, info, warn};

use crate::can::bitrate::{AdapterBitTiming, AdapterTimingConst, BitTimingConst, BitrateConfig};
use crate::can::{AsyncCanAdapter, CanAdapter, Frame, Identifier, DLC_TO_LEN};
use crate::Result;

/// PCAN-USB FD controller runs on a configurable clock; 80 MHz gives the
/// widest tseg range and is supported by every PCAN-USB FD device.
const DEFAULT_FD_CLOCK_MHZ: u32 = 80;

/// Hardware bit-timing limits used by [`crate::can::bitrate::BitrateBuilder`]
/// when solving timing for a PCAN-USB FD channel.
pub const PCAN_USB_FD_TIMING_CONST: AdapterTimingConst = AdapterTimingConst {
    nominal: BitTimingConst {
        clock_hz: 80_000_000,
        tseg1_min: 1,
        tseg1_max: 256,
        tseg2_min: 1,
        tseg2_max: 128,
        sjw_max: 128,
        brp_min: 1,
        brp_max: 1024,
        brp_inc: 1,
    },
    data: Some(BitTimingConst {
        clock_hz: 80_000_000,
        tseg1_min: 1,
        tseg1_max: 32,
        tseg2_min: 1,
        tseg2_max: 16,
        sjw_max: 16,
        brp_min: 1,
        brp_max: 1024,
        brp_inc: 1,
    }),
};

/// Blocking PCAN-USB adapter implementing [`CanAdapter`].
pub struct Pcan {
    channel: sys::TPCANHandle,
    /// Synthetic loopback frames queued on successful transmit.
    loopback_queue: VecDeque<Frame>,
}

impl Pcan {
    /// Convenience wrapper that opens the adapter and hands it to
    /// [`AsyncCanAdapter::new`].
    ///
    /// `channel_idx` is the zero-based PCAN-USB channel index
    /// (`0 -> PCAN_USBBUS1`, `7 -> PCAN_USBBUS8`).
    pub fn new_async(channel_idx: usize, bitrate_cfg: BitrateConfig) -> Result<AsyncCanAdapter> {
        Ok(AsyncCanAdapter::new(Self::new(channel_idx, bitrate_cfg)?))
    }

    /// Open a specific PCAN-USB channel with the provided bitrate config.
    ///
    /// Returns [`crate::Error::NotFound`] if the channel is not available,
    /// [`Error::UnsupportedBitrate`] if the bitrate cannot be mapped to a
    /// PCAN-Basic configuration, and [`Error::Initialize`] for driver-level
    /// initialization failures.
    pub fn new(channel_idx: usize, bitrate_cfg: BitrateConfig) -> Result<Pcan> {
        if channel_idx > (sys::PCAN_USBBUS_LAST - sys::PCAN_USBBUS1) as usize {
            return Err(crate::Error::NotSupported);
        }
        let channel = sys::PCAN_USBBUS1 + channel_idx as sys::TPCANHandle;

        let status = initialize_channel(channel, &bitrate_cfg)?;

        if status == sys::PCAN_ERROR_OK {
            let fd = bitrate_cfg.data.is_some();
            info!(
                channel = format_args!("0x{:02x}", channel),
                fd, "PCAN-USB channel opened"
            );
            configure_defaults(channel);
            Ok(Pcan {
                channel,
                loopback_queue: VecDeque::new(),
            })
        } else if matches!(
            status,
            sys::PCAN_ERROR_QRCVEMPTY | 0x00400 /* PCAN_ERROR_HWINUSE */
        ) || status == 0x00200
        /* PCAN_ERROR_NODRIVER */
        {
            Err(crate::Error::NotFound)
        } else {
            Err(Error::Initialize {
                channel,
                status: PcanStatus::from_code(status),
            }
            .into())
        }
    }

    /// Raw PCAN handle of the currently opened channel.
    pub fn channel(&self) -> u16 {
        self.channel
    }

    /// Read the PCAN controller status (`CAN_GetStatus`) as a
    /// [`PcanStatus`]. Useful for diagnosing bus-off and error-passive states.
    pub fn controller_status(&self) -> PcanStatus {
        let code = unsafe { sys::CAN_GetStatus(self.channel) };
        PcanStatus::from_code(code)
    }
}

impl Drop for Pcan {
    fn drop(&mut self) {
        unsafe {
            sys::CAN_Uninitialize(self.channel);
        }
    }
}

impl CanAdapter for Pcan {
    fn send(&mut self, frames: &mut VecDeque<Frame>) -> Result<()> {
        while let Some(frame) = frames.pop_front() {
            let msg = match build_msg(&frame) {
                Ok(msg) => msg,
                Err(err) => {
                    frames.push_front(frame);
                    return Err(err);
                }
            };

            let status = unsafe { sys::CAN_WriteFD(self.channel, &msg) };
            if status == sys::PCAN_ERROR_OK {
                let mut echo = frame;
                echo.loopback = true;
                self.loopback_queue.push_back(echo);
                continue;
            }

            if status & sys::PCAN_ERROR_QXMTFULL != 0 {
                // Hardware TX queue is full — retry on the next poll.
                frames.push_front(frame);
                break;
            }

            warn!(
                status = %PcanStatus::from_code(status),
                "PCAN: CAN_WriteFD failed"
            );
            frames.push_front(frame);
            return Err(Error::Driver(PcanStatus::from_code(status)).into());
        }
        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<Frame>> {
        let mut frames = Vec::new();
        loop {
            let mut msg = sys::TPCANMsgFD::zeroed();
            let mut ts: u64 = 0;
            let status = unsafe { sys::CAN_ReadFD(self.channel, &mut msg, &mut ts) };

            if status == sys::PCAN_ERROR_QRCVEMPTY {
                break;
            }

            if status != sys::PCAN_ERROR_OK {
                if status & !sys::PCAN_ERROR_QRCVEMPTY != 0 {
                    debug!(
                        status = %PcanStatus::from_code(status),
                        "PCAN: CAN_ReadFD non-ok status"
                    );
                }
                break;
            }

            if msg.msg_type
                & (sys::PCAN_MESSAGE_STATUS
                    | sys::PCAN_MESSAGE_ERRFRAME
                    | sys::PCAN_MESSAGE_RTR
                    | sys::PCAN_MESSAGE_ECHO)
                != 0
            {
                continue;
            }

            let id = if msg.msg_type & sys::PCAN_MESSAGE_EXTENDED != 0 {
                Identifier::Extended(msg.id)
            } else {
                Identifier::Standard(msg.id)
            };

            let dlc = msg.dlc as usize;
            if dlc >= DLC_TO_LEN.len() {
                warn!(dlc = msg.dlc, "PCAN: bogus DLC, skipping frame");
                continue;
            }
            let len = DLC_TO_LEN[dlc];

            frames.push(Frame {
                bus: 0,
                id,
                data: msg.data[..len].to_vec(),
                loopback: false,
                fd: msg.msg_type & sys::PCAN_MESSAGE_FD != 0,
            });
        }

        frames.extend(self.loopback_queue.drain(..));
        Ok(frames)
    }

    fn timing_const() -> AdapterTimingConst
    where
        Self: Sized,
    {
        PCAN_USB_FD_TIMING_CONST
    }
}

fn initialize_channel(
    channel: sys::TPCANHandle,
    bitrate_cfg: &BitrateConfig,
) -> Result<sys::TPCANStatus> {
    if let Some(data) = bitrate_cfg.data {
        let bitrate = build_bitrate_string(&bitrate_cfg.nominal, &data);
        let c_bitrate = CString::new(bitrate.clone())
            .map_err(|_| Error::UnsupportedBitrate(bitrate.clone()))?;
        Ok(unsafe { sys::CAN_InitializeFD(channel, c_bitrate.as_ptr()) })
    } else {
        let btr = classic_btr0_btr1(bitrate_cfg.nominal.bitrate)?;
        Ok(unsafe { sys::CAN_Initialize(channel, btr, 0, 0, 0) })
    }
}

/// Turn off everything that could introduce spurious frames in the receive
/// stream. Some PCBUSB builds ignore these, so failures are only logged.
fn configure_defaults(channel: sys::TPCANHandle) {
    let off = sys::PCAN_PARAMETER_OFF.to_le_bytes();
    for (name, param) in &[
        ("STATUS", sys::PCAN_ALLOW_STATUS_FRAMES),
        ("RTR", sys::PCAN_ALLOW_RTR_FRAMES),
        ("ERROR", sys::PCAN_ALLOW_ERROR_FRAMES),
        ("ECHO", sys::PCAN_ALLOW_ECHO_FRAMES),
    ] {
        let rc = unsafe {
            sys::CAN_SetValue(
                channel,
                *param,
                off.as_ptr() as *mut c_void,
                off.len() as u32,
            )
        };
        if rc != sys::PCAN_ERROR_OK {
            debug!(
                option = *name,
                status = %PcanStatus::from_code(rc),
                "PCAN: CAN_SetValue(OFF) rejected"
            );
        }
    }
}

fn build_bitrate_string(nominal: &AdapterBitTiming, data: &AdapterBitTiming) -> String {
    format!(
        "f_clock_mhz={clock},nom_brp={nbrp},nom_tseg1={nt1},nom_tseg2={nt2},nom_sjw={nsjw},\
         data_brp={dbrp},data_tseg1={dt1},data_tseg2={dt2},data_sjw={dsjw}",
        clock = DEFAULT_FD_CLOCK_MHZ,
        nbrp = nominal.brp,
        nt1 = nominal.tseg1,
        nt2 = nominal.tseg2,
        nsjw = nominal.sjw,
        dbrp = data.brp,
        dt1 = data.tseg1,
        dt2 = data.tseg2,
        dsjw = data.sjw,
    )
}

fn classic_btr0_btr1(bitrate_bps: u32) -> Result<u16> {
    let btr = match bitrate_bps {
        1_000_000 => sys::PCAN_BAUD_1M,
        800_000 => sys::PCAN_BAUD_800K,
        500_000 => sys::PCAN_BAUD_500K,
        250_000 => sys::PCAN_BAUD_250K,
        125_000 => sys::PCAN_BAUD_125K,
        100_000 => sys::PCAN_BAUD_100K,
        50_000 => sys::PCAN_BAUD_50K,
        20_000 => sys::PCAN_BAUD_20K,
        10_000 => sys::PCAN_BAUD_10K,
        other => {
            return Err(Error::UnsupportedBitrate(format!(
                "{other} bps is not supported in classic CAN mode; \
                 use one of 10k, 20k, 50k, 100k, 125k, 250k, 500k, 800k, 1M \
                 or provide a CAN-FD data bitrate"
            ))
            .into())
        }
    };
    Ok(btr)
}

fn build_msg(frame: &Frame) -> Result<sys::TPCANMsgFD> {
    let mut msg = sys::TPCANMsgFD::zeroed();

    let (id, extended) = match frame.id {
        Identifier::Standard(id) => (id, false),
        Identifier::Extended(id) => (id, true),
    };
    msg.id = id;

    let dlc = DLC_TO_LEN
        .iter()
        .position(|&x| x == frame.data.len())
        .ok_or(crate::Error::MalformedFrame)? as u8;
    msg.dlc = dlc;

    let mut flags = sys::PCAN_MESSAGE_STANDARD;
    if extended {
        flags |= sys::PCAN_MESSAGE_EXTENDED;
    }
    if frame.fd || frame.data.len() > 8 {
        flags |= sys::PCAN_MESSAGE_FD | sys::PCAN_MESSAGE_BRS;
    }
    msg.msg_type = flags;

    msg.data[..frame.data.len()].copy_from_slice(&frame.data);
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::can::bitrate::BitrateBuilder;

    #[test]
    fn fd_bitrate_string_matches_acam220_profile() {
        let cfg = BitrateBuilder::new::<Pcan>()
            .bitrate(500_000)
            .sample_point(0.8)
            .sjw(1)
            .data_bitrate(2_000_000)
            .data_sample_point(0.8)
            .data_sjw(1)
            .build()
            .unwrap();
        let data = cfg.data.unwrap();
        let bitrate_string = build_bitrate_string(&cfg.nominal, &data);
        // 80 MHz / (brp=1) / (1 + 127 + 32) = 500_000 bps
        assert!(bitrate_string.starts_with("f_clock_mhz=80,"));
        assert!(bitrate_string.contains("nom_brp=1,nom_tseg1=127,nom_tseg2=32,nom_sjw=1,"));
        assert!(bitrate_string.contains("data_brp=1,data_tseg1=31,data_tseg2=8,data_sjw=1"));
    }

    #[test]
    fn classic_bitrate_maps_to_pcan_constant() {
        assert_eq!(classic_btr0_btr1(500_000).unwrap(), sys::PCAN_BAUD_500K);
        assert_eq!(classic_btr0_btr1(125_000).unwrap(), sys::PCAN_BAUD_125K);
        assert!(matches!(
            classic_btr0_btr1(333_333),
            Err(crate::Error::PcanError(Error::UnsupportedBitrate(_)))
        ));
    }
}
