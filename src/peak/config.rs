use super::Peak;
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
    pub fn builder() -> PeakConfigBuilder {
        PeakConfigBuilder {
            bitrate: BitrateBuilder::new::<Peak>(),
        }
    }
}

/// Builder for a PEAK adapter configuration.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct PeakConfigBuilder {
    bitrate: BitrateBuilder,
}

impl PeakConfigBuilder {
    /// Configure bus timing using PEAK's timing capabilities.
    pub fn bitrate(mut self, configure: impl FnOnce(BitrateBuilder) -> BitrateBuilder) -> Self {
        self.bitrate = configure(self.bitrate);
        self
    }

    /// Calculate bus timing and build the adapter configuration.
    pub fn build(self) -> Result<PeakConfig, BitrateError> {
        Ok(PeakConfig::new(self.bitrate.build()?))
    }
}
