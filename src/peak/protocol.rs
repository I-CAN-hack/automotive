//! Encoding and decoding of the uCAN wire format used by the PCAN-USB FD family.
//!
//! The device exchanges fixed 8-byte *command* records on the command endpoint
//! and variable length *message* records (CAN frames, status, ...) on the
//! message endpoints. Both are little-endian.

use crate::can::bitrate::AdapterBitTiming;
use crate::can::{Frame, FrameContent, Identifier, DLC_TO_LEN};
use crate::peak::constants::*;
use crate::peak::error::Error;

/// Round `n` up to the next multiple of 4.
fn align4(n: usize) -> usize {
    (n + 3) & !3
}

/// Build the 16-bit `opcode_channel` field shared by every command/message.
fn opcode_channel(channel: u8, opcode: u16) -> u16 {
    ((channel as u16) << 12) | (opcode & 0x3ff)
}

/// Convert a DLC back to a payload length, honoring the CAN-FD lookup table for
/// FD frames and clamping classic frames to 8 bytes.
fn dlc_to_len(dlc: u8, fd: bool) -> usize {
    let dlc = (dlc & 0xf) as usize;
    if fd {
        DLC_TO_LEN[dlc]
    } else {
        DLC_TO_LEN[dlc].min(8)
    }
}

/// A single 8-byte command record.
pub type Command = [u8; COMMAND_SIZE];

fn command(channel: u8, opcode: u16) -> Command {
    let mut cmd = [0u8; COMMAND_SIZE];
    cmd[0..2].copy_from_slice(&opcode_channel(channel, opcode).to_le_bytes());
    cmd
}

/// The "end of collection" marker terminating a command list.
pub fn end_of_collection() -> Command {
    [0xff; COMMAND_SIZE]
}

/// Set the CAN controller clock domain (e.g. [`CLOCK_80MHZ`]).
pub fn cmd_set_clock(channel: u8, clock_mode: u8) -> Command {
    let mut cmd = command(channel, CMD_CLOCK_SET);
    cmd[2] = clock_mode;
    cmd
}

/// Enter reset mode (bus off). Bit timing can only be changed while in reset.
pub fn cmd_reset_mode(channel: u8) -> Command {
    command(channel, CMD_RESET_MODE)
}

/// Enter operational mode (bus on).
pub fn cmd_normal_mode(channel: u8) -> Command {
    command(channel, CMD_NORMAL_MODE)
}

/// Abort controller TX and discard its queued frames (uCAN TX_ABORT/FLUSH).
pub fn cmd_abort_tx(channel: u8) -> Command {
    let mut cmd = command(channel, CMD_TX_ABORT);
    cmd[2..4].copy_from_slice(&TX_ABORT_FLUSH.to_le_bytes());
    cmd
}

/// Reset both error counters to zero.
pub fn cmd_reset_error_counters(channel: u8) -> Command {
    let mut cmd = command(channel, CMD_WR_ERR_CNT);
    let sel_mask = WRERRCNT_TX_ENABLE | WRERRCNT_RX_ENABLE;
    cmd[2..4].copy_from_slice(&sel_mask.to_le_bytes());
    // tx_counter (byte 4) and rx_counter (byte 5) stay zero.
    cmd
}

/// Enable or disable the ISO CAN-FD framing option.
pub fn cmd_set_fd_iso(channel: u8, iso: bool) -> Command {
    let opcode = if iso {
        CMD_SET_EN_OPTION
    } else {
        CMD_CLR_DIS_OPTION
    };
    let mut cmd = command(channel, opcode);
    cmd[2..4].copy_from_slice(&OPTION_CAN_FD_ISO.to_le_bytes());
    cmd
}

/// Accept-all entry for one of the 64 standard-ID filter rows.
pub fn cmd_filter_std_row(channel: u8, idx: u16, mask: u32) -> Command {
    let mut cmd = command(channel, CMD_FILTER_STD);
    cmd[2..4].copy_from_slice(&idx.to_le_bytes());
    cmd[4..8].copy_from_slice(&mask.to_le_bytes());
    cmd
}

/// Number of standard-ID filter rows (each row covers 32 consecutive IDs).
pub const FILTER_STD_ROWS: u16 = 64;

/// Build the commands accepting every standard CAN ID.
pub fn cmd_filter_accept_all(channel: u8) -> Vec<Command> {
    (0..FILTER_STD_ROWS)
        .map(|idx| cmd_filter_std_row(channel, idx, 0xffff_ffff))
        .collect()
}

/// Build the slow (nominal/arbitration) bit-timing command.
pub fn cmd_timing_slow(channel: u8, t: &AdapterBitTiming) -> Command {
    let mut cmd = command(channel, CMD_TIMING_SLOW);
    cmd[2] = DEFAULT_ERROR_WARNING_LIMIT;
    cmd[3] = ((t.sjw - 1) & 0x7f) as u8; // sjw_t, triple-sampling bit left clear
    cmd[4] = ((t.tseg2 - 1) & 0x7f) as u8;
    cmd[5] = ((t.tseg1 - 1) & 0xff) as u8;
    cmd[6..8].copy_from_slice(&(((t.brp - 1) & 0x3ff) as u16).to_le_bytes());
    cmd
}

/// Build the fast (CAN-FD data phase) bit-timing command.
pub fn cmd_timing_fast(channel: u8, t: &AdapterBitTiming) -> Command {
    let mut cmd = command(channel, CMD_TIMING_FAST);
    // byte 2 unused
    cmd[3] = ((t.sjw - 1) & 0xf) as u8;
    cmd[4] = ((t.tseg2 - 1) & 0xf) as u8;
    cmd[5] = ((t.tseg1 - 1) & 0x1f) as u8;
    cmd[6..8].copy_from_slice(&(((t.brp - 1) & 0x3ff) as u16).to_le_bytes());
    cmd
}

/// Encode one CAN frame into a transmit message record.
///
/// `brs_supported` indicates whether a data bitrate is configured. The device
/// echoes transmitted frames (`LOOPED_BACK`) so the async layer can confirm TX.
pub fn encode_tx_frame(frame: &Frame, brs_supported: bool) -> Result<Vec<u8>, Error> {
    let dlc = frame.dlc().ok_or(Error::UnsupportedFrame)?;

    let mut flags = FLAG_LOOPED_BACK;
    let id = frame.id().ok_or(Error::UnsupportedFrame)?;
    if id.is_extended() {
        flags |= FLAG_EXT_ID;
    }
    let id = id.as_raw();

    if let FrameContent::Fd(fd) = frame.content() {
        flags |= FLAG_EXT_DATA_LEN;
        if fd.brs() {
            if !brs_supported {
                return Err(Error::MissingDataBitrate);
            }
            flags |= FLAG_BITRATE_SWITCH;
        }
        if fd.esi() {
            flags |= FLAG_ERROR_STATE_IND;
        }
    } else if frame.is_remote() {
        flags |= FLAG_RTR;
    }

    let msg_size = align4(TX_MSG_HEADER_SIZE + frame.data().len());
    let mut buf = vec![0u8; msg_size];

    buf[0..2].copy_from_slice(&(msg_size as u16).to_le_bytes());
    buf[2..4].copy_from_slice(&MSG_CAN_TX.to_le_bytes());
    // bytes 4..12: tag (unused, left zero)
    buf[12] = (frame.bus & 0xf) | (dlc << 4); // channel_dlc
                                              // byte 13: client (echo index, unused)
    buf[14..16].copy_from_slice(&flags.to_le_bytes());
    buf[16..20].copy_from_slice(&id.to_le_bytes());
    buf[20..20 + frame.data().len()].copy_from_slice(frame.data());

    Ok(buf)
}

/// Parse a buffer of received message records into CAN frames.
///
/// Non-CAN records (status, error, calibration, ...) are skipped. Parsing stops
/// at the first zero-size record (end of list) or when a record would run past
/// the end of the buffer. Returns the parsed frames and the number of dropped
/// frames reported by overrun records.
pub fn parse_rx_buffer(data: &[u8]) -> (Vec<Frame>, u32) {
    let mut frames = Vec::new();
    let mut overruns = 0u32;
    let mut off = 0;

    while off + 4 <= data.len() {
        let size = u16::from_le_bytes([data[off], data[off + 1]]) as usize;
        if size == 0 {
            break; // end of list marker
        }
        let msg_type = u16::from_le_bytes([data[off + 2], data[off + 3]]);

        if size < 4 || off + size > data.len() {
            break; // truncated / malformed record
        }

        match msg_type {
            MSG_CAN_RX if size >= RX_MSG_HEADER_SIZE => {
                let channel_dlc = data[off + 20];
                let flags = u16::from_le_bytes([data[off + 22], data[off + 23]]);
                let id = u32::from_le_bytes([
                    data[off + 24],
                    data[off + 25],
                    data[off + 26],
                    data[off + 27],
                ]);

                let dlc = channel_dlc >> 4;
                let channel = channel_dlc & 0xf;
                let fd = flags & FLAG_EXT_DATA_LEN != 0;
                let rtr = flags & FLAG_RTR != 0;
                let loopback = flags & FLAG_LOOPED_BACK != 0;

                let id = if flags & FLAG_EXT_ID != 0 {
                    Identifier::extended(id & 0x1fff_ffff).unwrap()
                } else {
                    Identifier::standard(id & 0x7ff).unwrap()
                };

                let parsed = if rtr && !fd {
                    Frame::remote_builder(id, dlc)
                        .bus(channel)
                        .loopback(loopback)
                        .build()
                } else if rtr {
                    Err(crate::Error::MalformedFrame)
                } else {
                    let want = dlc_to_len(dlc, fd);
                    let avail = size - RX_MSG_HEADER_SIZE;
                    if want > avail {
                        off += align4(size);
                        continue;
                    }
                    let builder = Frame::builder(id)
                        .bus(channel)
                        .loopback(loopback)
                        .data(&data[off + RX_MSG_HEADER_SIZE..off + RX_MSG_HEADER_SIZE + want]);
                    if fd {
                        builder
                            .fd()
                            .brs(flags & FLAG_BITRATE_SWITCH != 0)
                            .esi(flags & FLAG_ERROR_STATE_IND != 0)
                            .build()
                    } else {
                        builder.build()
                    }
                };
                if let Ok(frame) = parsed {
                    frames.push(frame);
                }
            }
            MSG_OVERRUN => overruns += 1,
            _ => {} // status / error / calibration / unknown: ignore
        }

        off += align4(size);
    }

    (frames, overruns)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(brp: u32, tseg1: u32, tseg2: u32, sjw: u32) -> AdapterBitTiming {
        AdapterBitTiming {
            brp,
            tseg1,
            tseg2,
            sjw,
            bitrate: 0,
            sample_point: 0.0,
        }
    }

    #[test]
    fn opcode_channel_packs_index_and_opcode() {
        assert_eq!(opcode_channel(0, CMD_TIMING_SLOW), 0x004);
        assert_eq!(opcode_channel(1, CMD_NORMAL_MODE), 0x1002);
        // Extended commands keep their opcode in the low bits.
        assert_eq!(opcode_channel(0, CMD_CLOCK_SET), 0x080);
    }

    #[test]
    fn timing_slow_subtracts_one_from_each_field() {
        // brp=8, tseg1=15, tseg2=4, sjw=2 -> 500k @ 80MHz, sp 0.8
        let cmd = cmd_timing_slow(0, &timing(8, 15, 4, 2));
        assert_eq!(u16::from_le_bytes([cmd[0], cmd[1]]), 0x004);
        assert_eq!(cmd[2], DEFAULT_ERROR_WARNING_LIMIT);
        assert_eq!(cmd[3], 1); // sjw - 1
        assert_eq!(cmd[4], 3); // tseg2 - 1
        assert_eq!(cmd[5], 14); // tseg1 - 1
        assert_eq!(u16::from_le_bytes([cmd[6], cmd[7]]), 7); // brp - 1
    }

    #[test]
    fn timing_fast_subtracts_one_from_each_field() {
        let cmd = cmd_timing_fast(0, &timing(4, 6, 3, 1));
        assert_eq!(u16::from_le_bytes([cmd[0], cmd[1]]), 0x005);
        assert_eq!(cmd[3], 0); // sjw - 1
        assert_eq!(cmd[4], 2); // tseg2 - 1
        assert_eq!(cmd[5], 5); // tseg1 - 1
        assert_eq!(u16::from_le_bytes([cmd[6], cmd[7]]), 3); // brp - 1
    }

    #[test]
    fn accept_all_filter_covers_every_standard_id() {
        let rows = cmd_filter_accept_all(0);
        assert_eq!(rows.len(), 2048 / 32);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(u16::from_le_bytes([row[0], row[1]]), CMD_FILTER_STD);
            assert_eq!(u16::from_le_bytes([row[2], row[3]]), i as u16);
            assert_eq!(
                u32::from_le_bytes([row[4], row[5], row[6], row[7]]),
                0xffff_ffff
            );
        }
    }

    #[test]
    fn encode_standard_frame_sets_looped_back() {
        let frame = Frame::new(
            0,
            Identifier::standard(0x123).unwrap(),
            &[1, 2, 3, 4, 5, 6, 7, 8],
        )
        .unwrap();
        let buf = encode_tx_frame(&frame, false).unwrap();

        assert_eq!(buf.len(), align4(TX_MSG_HEADER_SIZE + 8));
        assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), buf.len() as u16);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), MSG_CAN_TX);
        assert_eq!(buf[12], 8 << 4); // dlc 8, channel 0
        let flags = u16::from_le_bytes([buf[14], buf[15]]);
        assert_eq!(flags & FLAG_LOOPED_BACK, FLAG_LOOPED_BACK);
        assert_eq!(flags & FLAG_EXT_ID, 0);
        assert_eq!(
            u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
            0x123
        );
        assert_eq!(&buf[20..28], &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn encode_extended_fd_frame_sets_flags() {
        let frame = Frame::builder(Identifier::extended(0x1234).unwrap())
            .fd()
            .brs(true)
            .data([0xAA; 16])
            .build()
            .unwrap();
        assert!(frame.is_fd());
        let buf = encode_tx_frame(&frame, true).unwrap();

        let flags = u16::from_le_bytes([buf[14], buf[15]]);
        assert_eq!(flags & FLAG_EXT_ID, FLAG_EXT_ID);
        assert_eq!(flags & FLAG_EXT_DATA_LEN, FLAG_EXT_DATA_LEN);
        assert_eq!(flags & FLAG_BITRATE_SWITCH, FLAG_BITRATE_SWITCH);
        // dlc for 16 bytes is 0xA
        assert_eq!(buf[12] >> 4, 0xA);
    }

    #[test]
    fn encode_rejects_error_frames() {
        let frame = Frame::error_builder(crate::can::ErrorFlag::Active)
            .build()
            .unwrap();
        assert!(matches!(
            encode_tx_frame(&frame, false),
            Err(Error::UnsupportedFrame)
        ));
    }

    #[test]
    fn remote_requests_keep_dlc_and_have_no_payload() {
        for dlc in 0..=15 {
            let frame = Frame::remote_builder(Identifier::standard(0x123).unwrap(), dlc)
                .build()
                .unwrap();
            let tx = encode_tx_frame(&frame, false).unwrap();
            assert_eq!(tx.len(), TX_MSG_HEADER_SIZE);
            assert_eq!(tx[12] >> 4, dlc);
            assert_ne!(u16::from_le_bytes([tx[14], tx[15]]) & FLAG_RTR, 0);
            let (decoded, _) = parse_rx_buffer(&build_rx_record(&frame));
            assert_eq!(decoded, vec![frame]);
        }
    }

    #[test]
    fn fd_flags_are_preserved_and_brs_requires_data_timing() {
        let frame = Frame::builder(Identifier::standard(0x123).unwrap())
            .fd()
            .brs(true)
            .esi(true)
            .data([0xaa; 4])
            .build()
            .unwrap();
        assert!(matches!(
            encode_tx_frame(&frame, false),
            Err(Error::MissingDataBitrate)
        ));
        let tx = encode_tx_frame(&frame, true).unwrap();
        let flags = u16::from_le_bytes([tx[14], tx[15]]);
        assert_ne!(flags & FLAG_BITRATE_SWITCH, 0);
        assert_ne!(flags & FLAG_ERROR_STATE_IND, 0);
        let (decoded, _) = parse_rx_buffer(&build_rx_record(&frame));
        assert_eq!(decoded, vec![frame]);

        let frame = Frame::builder(Identifier::standard(0x123).unwrap())
            .fd()
            .build()
            .unwrap();
        let tx = encode_tx_frame(&frame, true).unwrap();
        assert_eq!(
            u16::from_le_bytes([tx[14], tx[15]]) & FLAG_BITRATE_SWITCH,
            0
        );
    }

    #[test]
    fn fd_remote_requests_are_rejected_on_receive() {
        let frame = Frame::remote_builder(Identifier::standard(0x123).unwrap(), 8)
            .build()
            .unwrap();
        let mut record = build_rx_record(&frame);
        let flags = FLAG_RTR | FLAG_EXT_DATA_LEN;
        record[22..24].copy_from_slice(&flags.to_le_bytes());
        assert!(parse_rx_buffer(&record).0.is_empty());
    }

    /// Build a received CAN record the way the device would, for round-trip tests.
    fn build_rx_record(frame: &Frame) -> Vec<u8> {
        let dlc = frame.dlc().unwrap();
        let size = align4(RX_MSG_HEADER_SIZE + frame.data().len());
        let mut buf = vec![0u8; size];

        let mut flags = 0u16;
        if frame.loopback {
            flags |= FLAG_LOOPED_BACK;
        }
        let id = frame.id().unwrap();
        if id.is_extended() {
            flags |= FLAG_EXT_ID;
        }
        let id = id.as_raw();
        if let FrameContent::Fd(fd) = frame.content() {
            flags |= FLAG_EXT_DATA_LEN;
            if fd.brs() {
                flags |= FLAG_BITRATE_SWITCH;
            }
            if fd.esi() {
                flags |= FLAG_ERROR_STATE_IND;
            }
        } else if frame.is_remote() {
            flags |= FLAG_RTR;
        }

        buf[0..2].copy_from_slice(&(size as u16).to_le_bytes());
        buf[2..4].copy_from_slice(&MSG_CAN_RX.to_le_bytes());
        buf[20] = (frame.bus & 0xf) | (dlc << 4);
        buf[22..24].copy_from_slice(&flags.to_le_bytes());
        buf[24..28].copy_from_slice(&id.to_le_bytes());
        buf[28..28 + frame.data().len()].copy_from_slice(frame.data());
        buf
    }

    #[test]
    fn decode_single_standard_frame() {
        let frame = Frame::new(
            0,
            Identifier::standard(0x123).unwrap(),
            &[1, 2, 3, 4, 5, 6, 7, 8],
        )
        .unwrap();
        let record = build_rx_record(&frame);

        let (frames, overruns) = parse_rx_buffer(&record);
        assert_eq!(overruns, 0);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], frame);
    }

    #[test]
    fn decode_multiple_records_and_stops_at_terminator() {
        let f1 = Frame::new(0, Identifier::standard(0x100).unwrap(), &[0xDE, 0xAD]).unwrap();
        let f2 = Frame::new(
            0,
            Identifier::extended(0x1abcd).unwrap(),
            &[0xBE, 0xEF, 0x00, 0x11],
        )
        .unwrap();

        let mut buf = build_rx_record(&f1);
        buf.extend_from_slice(&build_rx_record(&f2));
        buf.extend_from_slice(&[0u8; 4]); // zero terminator
        buf.extend_from_slice(&[0xAA; 8]); // garbage after terminator, must be ignored

        let (frames, _) = parse_rx_buffer(&buf);
        assert_eq!(frames, vec![f1, f2]);
    }

    #[test]
    fn decode_marks_loopback_frames() {
        let mut frame = Frame::new(0, Identifier::standard(0x321).unwrap(), &[0xCA, 0xFE]).unwrap();
        frame.loopback = true;
        let record = build_rx_record(&frame);

        let (frames, _) = parse_rx_buffer(&record);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].loopback);
        assert_eq!(frames[0], frame);
    }

    #[test]
    fn encode_decode_round_trip() {
        let frames = vec![
            Frame::new(
                0,
                Identifier::standard(0x123).unwrap(),
                &[1, 2, 3, 4, 5, 6, 7, 8],
            )
            .unwrap(),
            Frame::new(0, Identifier::extended(0x1234).unwrap(), &[0xAA; 8]).unwrap(),
            Frame::new(0, Identifier::standard(0x0).unwrap(), &[]).unwrap(),
            Frame::new(0, Identifier::extended(0x18DAF110).unwrap(), &[0u8; 64]).unwrap(),
        ];

        for frame in frames {
            // Encode as TX, then re-shape into the RX layout the device echoes back.
            let tx = encode_tx_frame(&frame, true).unwrap();
            assert_eq!(u16::from_le_bytes([tx[2], tx[3]]), MSG_CAN_TX);

            let mut echo = frame.clone();
            echo.loopback = true;
            let record = build_rx_record(&echo);
            let (decoded, _) = parse_rx_buffer(&record);
            assert_eq!(decoded.len(), 1);
            assert_eq!(decoded[0], echo);
        }
    }

    #[test]
    fn decode_truncated_record_is_ignored() {
        let frame = Frame::new(
            0,
            Identifier::standard(0x123).unwrap(),
            &[1, 2, 3, 4, 5, 6, 7, 8],
        )
        .unwrap();
        let mut record = build_rx_record(&frame);
        record.truncate(record.len() - 4); // cut off part of the payload

        let (frames, _) = parse_rx_buffer(&record);
        assert!(frames.is_empty());
    }
}
