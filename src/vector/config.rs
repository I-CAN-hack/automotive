use super::VectorCan;
use crate::can::bitrate::{BitrateBuilder, BitrateConfig, BitrateError};

/// Access requested when opening a Vector XL CAN channel.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum VectorAccess {
    /// Open without initialization access, retaining the existing bus timing.
    #[default]
    Shared,
    /// Request initialization access and configure the bus timing.
    Init { bitrate: BitrateConfig },
}

/// Configuration for a Vector XL CAN channel.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct VectorConfig {
    /// Global channel index reported by the XL driver.
    pub channel: usize,
    /// Shared access retains existing timing; initialization requires a bitrate.
    pub access: VectorAccess,
}

impl VectorConfig {
    /// Open the selected channel with shared access, retaining its bus timing.
    pub fn new(channel: usize) -> Self {
        Self {
            channel,
            ..Default::default()
        }
    }

    /// Build a configuration for the selected channel, using shared access
    /// unless bus timing is supplied through the builder.
    pub fn builder(channel: usize) -> VectorConfigBuilder {
        VectorConfigBuilder {
            channel,
            bitrate: None,
        }
    }
}

/// Builder for a Vector XL channel configuration.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct VectorConfigBuilder {
    channel: usize,
    bitrate: Option<BitrateBuilder>,
}

impl VectorConfigBuilder {
    /// Request initialization access and configure bus timing using Vector's
    /// timing capabilities. Omitting this call retains shared access.
    pub fn bitrate(mut self, configure: impl FnOnce(BitrateBuilder) -> BitrateBuilder) -> Self {
        let bitrate = self
            .bitrate
            .unwrap_or_else(BitrateBuilder::new::<VectorCan>);
        self.bitrate = Some(configure(bitrate));
        self
    }

    /// Calculate any supplied bus timing and build the channel configuration.
    pub fn build(self) -> Result<VectorConfig, BitrateError> {
        let access = match self.bitrate {
            Some(bitrate) => VectorAccess::Init {
                bitrate: bitrate.build()?,
            },
            None => VectorAccess::Shared,
        };
        Ok(VectorConfig {
            channel: self.channel,
            access,
        })
    }
}
