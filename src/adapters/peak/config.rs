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
    /// Allow detaching a kernel driver to claim the USB interface.
    /// Defaults to false, so opening fails if the kernel driver owns it.
    /// When enabled, the driver is reattached when the adapter is released.
    #[cfg_attr(feature = "serde", serde(default))]
    pub detach_kernel_driver: bool,
}

impl PeakConfig {
    /// Create a configuration with the required bus timing.
    /// Kernel driver detachment is disabled by default.
    pub fn new(bitrate: BitrateConfig) -> Self {
        Self {
            bitrate,
            tx_timeout: default_tx_timeout(),
            detach_kernel_driver: false,
        }
    }

    /// Build a configuration using PEAK's bit timing capabilities.
    /// Omitting bus timing uses 500 kbit/s nominal and 2 Mbit/s data, both with
    /// an 80% sample point. Kernel driver detachment is disabled by default.
    pub fn builder() -> PeakConfigBuilder {
        PeakConfigBuilder {
            bitrate: None,
            tx_timeout: default_tx_timeout(),
            detach_kernel_driver: false,
        }
    }
}

/// Builder for a PEAK adapter configuration.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct PeakConfigBuilder {
    bitrate: Option<BitrateBuilder>,
    tx_timeout: Option<Duration>,
    detach_kernel_driver: bool,
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

    /// Allow detaching a kernel driver to claim the USB interface.
    /// Disabled by default. Enabling this interrupts SocketCAN access until
    /// the adapter is released and the kernel driver is reattached.
    pub fn detach_kernel_driver(mut self, detach: bool) -> Self {
        self.detach_kernel_driver = detach;
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
            detach_kernel_driver: self.detach_kernel_driver,
        })
    }
}
