use crate::can::Frame;
use crate::can::Identifier;
use crate::error::Error;

static CANPACKET_HEAD_SIZE: usize = 0x6;
static DLC_TO_LEN: &'static [usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];

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
        let id: Identifier = id.into(); // TODO: Handle extended identifiers explicitly

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
        });

        dat.drain(0..(CANPACKET_HEAD_SIZE + data_len));
    }

    return Ok(ret);
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
}
