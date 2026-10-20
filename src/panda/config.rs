use super::Panda;
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
    pub fn builder() -> PandaConfigBuilder {
        PandaConfigBuilder {
            bitrate: BitrateBuilder::new::<Panda>(),
        }
    }
}

/// Builder for a panda adapter configuration.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct PandaConfigBuilder {
    bitrate: BitrateBuilder,
}

impl PandaConfigBuilder {
    /// Configure bus timing using panda's timing capabilities.
    pub fn bitrate(mut self, configure: impl FnOnce(BitrateBuilder) -> BitrateBuilder) -> Self {
        self.bitrate = configure(self.bitrate);
        self
    }

    /// Calculate bus timing and build the adapter configuration.
    pub fn build(self) -> Result<PandaConfig, BitrateError> {
        Ok(PandaConfig::new(self.bitrate.build()?))
    }
}
