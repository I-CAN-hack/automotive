use super::Peak;
use crate::can::adapter::default_bitrate;
use crate::can::bitrate::{BitrateBuilder, BitrateConfig, BitrateError};

/// Configuration for a PEAK PCAN-USB FD adapter.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PeakConfig {
    pub bitrate: BitrateConfig,
}

impl PeakConfig {
    /// Create a configuration with the required bus timing.
    pub fn new(bitrate: BitrateConfig) -> Self {
        Self { bitrate }
    }

    /// Build a configuration using PEAK's bit timing capabilities.
    /// Omitting bus timing uses 500 kbit/s nominal and 2 Mbit/s data, both with
    /// an 80% sample point.
    pub fn builder() -> PeakConfigBuilder {
        PeakConfigBuilder { bitrate: None }
    }
}

/// Builder for a PEAK adapter configuration.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct PeakConfigBuilder {
    bitrate: Option<BitrateBuilder>,
}

impl PeakConfigBuilder {
    /// Configure bus timing using PEAK's timing capabilities, starting without
    /// preset timing. Setting only a nominal bitrate selects classic CAN.
    pub fn bitrate(mut self, configure: impl FnOnce(BitrateBuilder) -> BitrateBuilder) -> Self {
        let bitrate = self.bitrate.unwrap_or_else(BitrateBuilder::new::<Peak>);
        self.bitrate = Some(configure(bitrate));
        self
    }

    /// Calculate bus timing and build the adapter configuration.
    pub fn build(self) -> Result<PeakConfig, BitrateError> {
        let bitrate = self
            .bitrate
            .unwrap_or_else(|| default_bitrate(BitrateBuilder::new::<Peak>()));
        Ok(PeakConfig::new(bitrate.build()?))
    }
}
