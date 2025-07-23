pub enum AdapterConfig {
    /// Best effort adapter configuration. Will choose the first available adapter, and apply the configuration
    Any(GenericConfig),

    /// SocketCAN adapter configuration.
    /// We currently don't support setting the bitrate for SocketCAN, as it is usually set by the system.
    /// If no interface is provided, it will use the first available interface.
    SocketCAN(Option<String>),

    /// Panda adapter configuration. Requires a valid interface serial and bitrate configuration.
    /// If no serial is provided, it will use the first available interface.
    Panda(Option<String>, GenericConfig),

    /// Vector adapter configuration.
    /// If no channel is provided, it will use the first available global channel index.
    Vector(Option<VectorChannel>, VectorConfig),
}

pub enum VectorChannel {
    /// Identify a channel by it's global index
    ChannelIndex(u32),
    /// Identify a channel by it's application name and channel index
    Application(String, u32),
}

pub enum VectorConfig {
    /// Open channel without requesting init access (non-exclusive mode).
    /// This allows for piggy-backing on the bus without forcing configuration, e.g. for simultaneous use with CANoe.
    NonInitAccess,
    /// Open channel while requesting init access. This allows us to configure the channel and set the bitrate.
    /// Other applications might still be able to use the channel in NonInitAccess mode.
    InitAccess(GenericConfig),
}

pub struct GenericConfig {
    pub classic: TimingConfig,
    /// If None, FD support will be disabled.
    pub fd: Option<TimingConfig>,
}

pub struct TimingConfig {
    /// The bitrate in bits per second
    pub bitrate: u32,
    /// Between 0 and 1, where 0 is 0% and 1 is 100% of the bit time.
    pub sample_point: f32,
}
