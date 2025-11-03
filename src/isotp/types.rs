use crate::can::Frame;

#[derive(Debug, Copy, Clone)]
pub struct FlowControlConfig {
    pub block_size: u8,
    pub separation_time_min: std::time::Duration,
}

impl TryFrom<&Frame> for FlowControlConfig {
    type Error = crate::error::Error;
    fn try_from(frame: &Frame) -> Result<Self, Self::Error> {
        if frame.data.len() < 3 {
            return Err(crate::isotp::error::Error::MalformedFrame.into());
        }

        let block_size = frame.data[1];

        let separation_time_min = frame.data[2] as u64;
        let separation_time_min = match separation_time_min {
            0x0..=0x7f => std::time::Duration::from_millis(separation_time_min),
            0xf1..=0xf9 => std::time::Duration::from_micros((separation_time_min - 0xf0) * 100),
            _ => return Err(crate::isotp::error::Error::MalformedFrame.into()),
        };

        Ok(Self {
            block_size,
            separation_time_min,
        })
    }
}
