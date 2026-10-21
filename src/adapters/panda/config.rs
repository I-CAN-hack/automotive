use super::Panda;
use crate::can::adapter::default_bitrate;
use crate::can::bitrate::{BitrateBuilder, BitrateConfig, BitrateError};

/// Configuration for a panda CAN adapter.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PandaConfig {
    pub bitrate: BitrateConfig,
}

impl PandaConfig {
    /// Create a configuration with the required bus timing.
    pub fn new(bitrate: BitrateConfig) -> Self {
        Self { bitrate }
    }

    /// Build a configuration using panda's bit timing capabilities.
    /// Omitting bus timing uses 500 kbit/s nominal and 2 Mbit/s data, both with
    /// an 80% sample point.
    pub fn builder() -> PandaConfigBuilder {
        PandaConfigBuilder { bitrate: None }
    }
}

/// Builder for a panda adapter configuration.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct PandaConfigBuilder {
    bitrate: Option<BitrateBuilder>,
}

impl PandaConfigBuilder {
    /// Configure bus timing using panda's timing capabilities, starting without
    /// preset timing. Setting only a nominal bitrate selects classic CAN.
    pub fn bitrate(mut self, configure: impl FnOnce(BitrateBuilder) -> BitrateBuilder) -> Self {
        let bitrate = self.bitrate.unwrap_or_else(BitrateBuilder::new::<Panda>);
        self.bitrate = Some(configure(bitrate));
        self
    }

    /// Calculate bus timing and build the adapter configuration.
    pub fn build(self) -> Result<PandaConfig, BitrateError> {
        let bitrate = self
            .bitrate
            .unwrap_or_else(|| default_bitrate(BitrateBuilder::new::<Panda>()));
        Ok(PandaConfig::new(bitrate.build()?))
    }
}
