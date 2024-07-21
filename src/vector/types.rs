use crate::{XLaccess, XLportHandle};

pub struct ApplicationConfig {
    pub hw_type: i32,
    pub hw_index: i32,
    pub hw_channel: i32,
}

#[derive(Clone)]
pub enum CanFilter {
    Standard { id: u32, mask: u32 },
    Extended { id: u32, mask: u32, extended: bool },
}

pub struct PortConfig {
    pub port_handle: XLportHandle,
    pub permission_mask: XLaccess,
}
