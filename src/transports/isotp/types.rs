use crate::can::Frame;

#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FlowControlConfig {
    pub block_size: u8,
    pub separation_time_min: std::time::Duration,
    #[cfg_attr(feature = "serde", serde(default))]
    pub reserved_stmin: bool,
}

impl TryFrom<&Frame> for FlowControlConfig {
    type Error = crate::error::Error;
    fn try_from(frame: &Frame) -> Result<Self, Self::Error> {
        if !frame.is_data() {
            return Err(crate::transports::isotp::error::Error::MalformedFrame.into());
        }
        Self::try_from(frame.data())
    }
}

impl TryFrom<&[u8]> for FlowControlConfig {
    type Error = crate::error::Error;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        if data.len() < 3 {
            return Err(crate::transports::isotp::error::Error::MalformedFrame.into());
        }

        let block_size = data[1];

        let separation_time_min = data[2] as u64;
        let reserved_stmin = matches!(data[2], 0x80..=0xf0 | 0xfa..=0xff);
        let separation_time_min = match separation_time_min {
            0x0..=0x7f => std::time::Duration::from_millis(separation_time_min),
            0xf1..=0xf9 => std::time::Duration::from_micros((separation_time_min - 0xf0) * 100),
            _ => std::time::Duration::from_millis(127),
        };

        Ok(Self {
            block_size,
            separation_time_min,
            reserved_stmin,
        })
    }
}
