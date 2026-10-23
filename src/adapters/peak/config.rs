use super::PeakAdapter;
use crate::adapters::{default_bitrate, default_tx_timeout};
use crate::can::bitrate::{BitrateBuilder, BitrateConfig, BitrateError};
use std::time::Duration;

/// Configuration for a PEAK PCAN-USB FD adapter.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PeakConfig {
    pub bitrate: BitrateConfig,
    /// Queue/confirmation deadline. Defaults to one second; `None` disables it.
    #[cfg_attr(
        feature = "serde",
        serde(default = "crate::adapters::default_tx_timeout")
    )]
    pub tx_timeout: Option<Duration>,
}

impl PeakConfig {
    /// Create a configuration with the required bus timing.
    pub fn new(bitrate: BitrateConfig) -> Self {
        Self {
            bitrate,
            tx_timeout: default_tx_timeout(),
        }
    }

    /// Build a configuration using PEAK's bit timing capabilities.
    /// Omitting bus timing uses 500 kbit/s nominal and 2 Mbit/s data, both with
    /// an 80% sample point.
    pub fn builder() -> PeakConfigBuilder {
        PeakConfigBuilder {
            bitrate: None,
            tx_timeout: default_tx_timeout(),
        }
    }
}

/// Builder for a PEAK adapter configuration.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct PeakConfigBuilder {
    bitrate: Option<BitrateBuilder>,
    tx_timeout: Option<Duration>,
}

impl PeakConfigBuilder {
    /// Configure bus timing using PEAK's timing capabilities, starting without
    /// preset timing. Setting only a nominal bitrate selects classic CAN.
    pub fn bitrate(mut self, configure: impl FnOnce(BitrateBuilder) -> BitrateBuilder) -> Self {
        let bitrate = self
            .bitrate
            .unwrap_or_else(BitrateBuilder::new::<PeakAdapter>);
        self.bitrate = Some(configure(bitrate));
        self
    }

    /// Set the queue/confirmation deadline for each transmission.
    pub fn tx_timeout(mut self, timeout: Duration) -> Self {
        self.tx_timeout = Some(timeout);
        self
    }

    /// Disable the default transmit deadline.
    pub fn without_tx_timeout(mut self) -> Self {
        self.tx_timeout = None;
        self
    }

    /// Calculate bus timing and build the adapter configuration.
    pub fn build(self) -> Result<PeakConfig, BitrateError> {
        let bitrate = self
            .bitrate
            .unwrap_or_else(|| default_bitrate(BitrateBuilder::new::<PeakAdapter>()));
        Ok(PeakConfig {
            bitrate: bitrate.build()?,
            tx_timeout: self.tx_timeout,
        })
    }
}
