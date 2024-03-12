use crate::can::Frame;
use crate::can::Identifier;
use crate::error::Error;

const CANPACKET_HEAD_SIZE: usize = 0x6;
const CANPACKET_MAX_CHUNK_SIZE: usize = 256;
static DLC_TO_LEN: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];

// Header layout

//  byte 0
//   unsigned char reserved : 1;
//   unsigned char bus : 3;
//   unsigned char data_len_code : 4;  // lookup length with dlc_to_len

// byte 1, 2, 3, 4
//   unsigned char rejected : 1;
//   unsigned char returned : 1;
//   unsigned char extended : 1;
//   unsigned int addr : 29;

// byte 5
//   unsigned char checksum;

// byte 6
//   unsigned char data[CANPACKET_DATA_SIZE_MAX];
// }

fn calculate_checksum(dat: &[u8]) -> u8 {
    dat.iter().fold(0, |acc, &x| acc ^ x)
}

pub fn pack_can_buffer(frames: &[Frame]) -> Result<Vec<Vec<u8>>, Error> {
    let mut ret = vec![];
    ret.push(vec![]);

    for frame in frames {
        let extended: u32 = match frame.id {
            Identifier::Standard(_) => 0,
            Identifier::Extended(_) => 1,
        };

        let id: u32 = frame.id.into();

        // Check if the id is valid
        if id > 0x7ff && extended == 0 {
            return Err(Error::MalformedFrame);
        }

        let dlc = DLC_TO_LEN.iter().position(|&x| x == frame.data.len());
        let dlc = dlc.ok_or(Error::MalformedFrame)? as u8;

        let word_4b: u32 = (id << 3) | (extended << 2);

        let header: [u8; CANPACKET_HEAD_SIZE - 1] = [
            (dlc << 4) | (frame.bus << 1),
            (word_4b & 0xff) as u8,
            ((word_4b >> 8) & 0xff) as u8,
            ((word_4b >> 16) & 0xff) as u8,
            ((word_4b >> 24) & 0xff) as u8,
        ];

        let checksum = calculate_checksum(&header) ^ calculate_checksum(&frame.data);

        let last = ret.last_mut().unwrap();
        last.extend_from_slice(&header);
        last.push(checksum);
        last.extend_from_slice(&frame.data);

        if last.len() > CANPACKET_MAX_CHUNK_SIZE {
            ret.push(vec![]);
        }
    }

    Ok(ret)
}

pub fn unpack_can_buffer(dat: &mut Vec<u8>) -> Result<Vec<Frame>, Error> {
    let mut ret = vec![];
    while dat.len() >= CANPACKET_HEAD_SIZE {
        let bus = (dat[0] >> 1) & 0b111;
        let dlc = (dat[0] >> 4) & 0b1111;
        let id: u32 = ((dat[4] as u32) << 24
            | (dat[3] as u32) << 16
            | (dat[2] as u32) << 8
            | (dat[1] as u32))
            >> 3;

        let extended: bool = (dat[1] & 0b100) != 0;
        let returned: bool = (dat[1] & 0b010) != 0;

        // Check if the id is valid
        if id > 0x7ff && !extended {
            return Err(Error::MalformedFrame);
        }

        let id = match extended {
            true => Identifier::Extended(id),
            false => Identifier::Standard(id),
        };

        // Check if we have enough data to unpack the whole frame
        let data_len = DLC_TO_LEN[dlc as usize];
        if data_len > dat.len() - CANPACKET_HEAD_SIZE {
            break;
        }

        if calculate_checksum(&dat[0..(CANPACKET_HEAD_SIZE + data_len)]) != 0 {
            return Err(Error::PandaError(
                crate::panda::error::Error::InvalidChecksum,
            ));
        }

        ret.push(Frame {
            id,
            bus,
            data: dat[CANPACKET_HEAD_SIZE..(CANPACKET_HEAD_SIZE + data_len)].to_vec(),
            returned,
        });

        dat.drain(0..(CANPACKET_HEAD_SIZE + data_len));
    }

    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unpack_single() {
        let mut buffer = vec![
            208, 128, 1, 0, 0, 171, 0, 0, 0, 0, 0, 0, 13, 69, 0, 0, 8, 0, 0, 27, 0, 0, 0, 0, 0, 1,
            0, 0, 255, 250, 0, 0, 0, 0, 199, 116, 151, 129,
        ];
        let frames = unpack_can_buffer(&mut buffer).unwrap();

        // All data is consumed
        assert_eq!(buffer.len(), 0);

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].id, Identifier::Standard(48));
        assert_eq!(frames[0].bus, 0);
        assert_eq!(
            frames[0].data,
            vec![
                0, 0, 0, 0, 0, 0, 13, 69, 0, 0, 8, 0, 0, 27, 0, 0, 0, 0, 0, 1, 0, 0, 255, 250, 0,
                0, 0, 0, 199, 116, 151, 129
            ]
        );
    }

    #[test]
    fn test_remaining_data() {
        let mut buffer = vec![
            208, 128, 1, 0, 0, 171, 0, 0, 0, 0, 0, 0, 13, 69, 0, 0, 8, 0, 0, 27, 0, 0, 0, 0, 0, 1,
            0, 0, 255, 250, 0, 0, 0, 0, 199, 116, 151, 129, // Extra
            208, 128,
        ];

        unpack_can_buffer(&mut buffer).unwrap();
        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn test_round_trip() {
        let frames = vec![
            Frame {
                bus: 0,
                id: Identifier::Standard(0x123),
                data: vec![1, 2, 3, 4, 5, 6, 7, 8],
                returned: false,
            },
            Frame {
                bus: 1,
                id: Identifier::Extended(0x123),
                data: vec![1, 2, 3, 4],
                returned: false,
            },
        ];

        let buffer = pack_can_buffer(&frames).unwrap();
        let mut buffer = buffer.concat();
        let unpacked = unpack_can_buffer(&mut buffer).unwrap();

        assert_eq!(frames, unpacked);
    }

    #[test]
    fn test_round_malformed_dlc() {
        let frames = vec![Frame {
            bus: 0,
            id: Identifier::Standard(0x123),
            data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
            returned: false,
        }];
        let r = pack_can_buffer(&frames);
        assert_eq!(r, Err(Error::MalformedFrame));
    }

    #[test]
    fn test_round_malformed_id() {
        let frames = vec![Frame {
            bus: 0,
            id: Identifier::Standard(0xfff),
            data: vec![1, 2, 3, 4, 5, 6, 7, 8],
            returned: false,
        }];
        let r = pack_can_buffer(&frames);
        assert_eq!(r, Err(Error::MalformedFrame));
    }
}
