use std::time::Duration;

/// Configuration for a [`super::UDSClient`].
///
/// P2 defaults to 50 ms and P2* to five seconds, based on the recommended server
/// timings in ISO 14229-2:2013, §7.2, Table 4. Configured and server-reported
/// timeouts are used directly.
///
/// The transport exposes complete messages, so these deadlines include response
/// reception (P6/P6* in ISO 14229-2 terminology), rather than just its start.
/// Transport timeouts still apply and should accommodate the configured waits.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct UDSConfig {
    /// Initial response wait. Defaults to 50 ms.
    pub p2_timeout: Duration,
    /// Response wait after each NRC 0x78.
    /// Defaults to five seconds; each pending response restarts this wait.
    pub p2_star_timeout: Duration,
    /// Overall limit, including sending and all pending responses.
    /// `None` (the default) permits any number of pending responses.
    pub request_timeout: Option<Duration>,
    /// Adopt P2/P2* reported by DiagnosticSessionControl. Defaults to `true`.
    /// A response without a timing record leaves the current timings unchanged.
    pub use_server_timing: bool,
    /// Enforce existing response echo and format checks. Defaults to `true`.
    /// Failures include the full response in [`super::Error::ProtocolDeviation`].
    /// Minimum lengths needed for safe parsing apply in both modes.
    pub strict: bool,
}

impl Default for UDSConfig {
    fn default() -> Self {
        Self {
            p2_timeout: Duration::from_millis(50),
            p2_star_timeout: Duration::from_secs(5),
            request_timeout: None,
            use_server_timing: true,
            strict: true,
        }
    }
}

impl UDSConfig {
    pub fn builder() -> UDSConfigBuilder {
        UDSConfigBuilder {
            config: Self::default(),
        }
    }
}

/// Builder for [`UDSConfig`]. Start with [`UDSConfig::builder`].
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct UDSConfigBuilder {
    config: UDSConfig,
}

impl UDSConfigBuilder {
    /// Set the initial response wait.
    pub fn p2_timeout(mut self, timeout: Duration) -> Self {
        self.config.p2_timeout = timeout;
        self
    }

    /// Set the wait restarted by each Response Pending (NRC 0x78).
    pub fn p2_star_timeout(mut self, timeout: Duration) -> Self {
        self.config.p2_star_timeout = timeout;
        self
    }

    /// Limit the entire exchange, including sending and pending responses.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.config.request_timeout = Some(timeout);
        self
    }

    /// Remove the overall limit. Individual response waits still apply.
    pub fn without_request_timeout(mut self) -> Self {
        self.config.request_timeout = None;
        self
    }

    /// Use server-reported session timings instead of the configured P2/P2*.
    pub fn use_server_timing(mut self, enabled: bool) -> Self {
        self.config.use_server_timing = enabled;
        self
    }

    /// Enable existing protocol checks. Safe parsing lengths always apply.
    pub fn strict(mut self, enabled: bool) -> Self {
        self.config.strict = enabled;
        self
    }

    pub fn build(self) -> UDSConfig {
        self.config
    }
}
