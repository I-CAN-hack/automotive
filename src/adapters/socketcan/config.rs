/// Configuration for an existing SocketCAN interface.
/// Bus timing is configured through the operating system.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SocketCanConfig {
    pub interface: String,
}

impl SocketCanConfig {
    /// Select an existing interface, such as `can0` or `vcan0`.
    pub fn new(interface: impl Into<String>) -> Self {
        Self {
            interface: interface.into(),
        }
    }

    /// Build a configuration for an existing SocketCAN interface.
    pub fn builder(interface: impl Into<String>) -> SocketCanConfigBuilder {
        SocketCanConfigBuilder {
            interface: interface.into(),
        }
    }
}

/// Builder for a SocketCAN interface configuration.
#[derive(Debug, Clone)]
#[must_use]
pub struct SocketCanConfigBuilder {
    interface: String,
}

impl SocketCanConfigBuilder {
    /// Build the interface configuration without changing bus timing.
    pub fn build(self) -> SocketCanConfig {
        SocketCanConfig::new(self.interface)
    }
}
