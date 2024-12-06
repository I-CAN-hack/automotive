const DEFAULT_BITRATE: u32 = 500_000;
const DEFAULT_DBITRATE: u32 = 2_000_000; // SAE J2284-4
const DEFAULT_SAMPLE_POINT: f32 = 0.8; // SAE J2284-4 and SAE J2284-5

pub struct TimingConfig {
    pub classic: BitTiming,
    pub fd: Option<BitTiming>,
}

pub struct BitTiming {
    pub bitrate: u32,
    pub sample_point: f32,
}

impl Default for TimingConfig {
    fn default() -> Self {
        TimingConfig {
            classic: BitTiming {
                bitrate: DEFAULT_BITRATE,
                sample_point: DEFAULT_SAMPLE_POINT,
            },
            fd: Some(BitTiming {
                bitrate: DEFAULT_DBITRATE,
                sample_point: DEFAULT_SAMPLE_POINT,
            }),
        }
    }
}
