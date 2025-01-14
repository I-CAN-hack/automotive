use crate::vector::bindings as xl;
use crate::vector::error::Error;
use crate::vector::types::{
    ChannelConfig, HwType, PortHandle, XLaccess, XLcanFdConf, XLcanRxEvent, XLcanTxEvent,
};
use crate::Result;

// CAN FD: Size of the port receive queue allocated by the driver in bytes. The value must be a
// power of 2 and within a range of 8192…524288 bytes (0.5 MB).
const DEFAULT_RX_QUEUE_SIZE: u32 = 52428;

pub fn xl_open_driver() -> Result<()> {
    let status = unsafe { xl::xlOpenDriver() };

    match status as u32 {
        xl::XL_SUCCESS => Ok(()),
        _ => Err(Error::DriverError(format!("xlOpenDriver failed, err {}", status)).into()),
    }
}

pub fn xl_close_driver() -> Result<()> {
    let status = unsafe { xl::xlCloseDriver() };

    match status as u32 {
        xl::XL_SUCCESS => Ok(()),
        _ => Err(Error::DriverError(format!("xlCloseDriver failed, err {}", status)).into()),
    }
}

pub fn xl_get_driver_config(channel_idx: usize) -> Result<ChannelConfig> {
    unsafe {
        let mut config: xl::XLdriverConfig = std::mem::zeroed();
        let status = xl::xlGetDriverConfig(&mut config);

        match status as u32 {
            xl::XL_SUCCESS => {
                let channel_count: usize = config.channelCount as usize;
                assert!(channel_idx < channel_count);
                tracing::info!("Channel count {}", channel_count);

                let channel = config.channel[channel_idx];

                Ok(ChannelConfig {
                    hw_type: HwType::from_repr(channel.hwType as u32).unwrap(),
                    hw_index: channel.hwIndex as u32,
                    hw_channel: channel.hwChannel as u32,
                })
            }
            _ => {
                Err(Error::DriverError(format!("xlGetDriverConfig failed, err {}", status)).into())
            }
        }
    }
}

#[allow(dead_code)]
pub fn xl_get_application_config(app_name: &str, app_channel: u32) -> Result<ChannelConfig> {
    unsafe {
        let mut hw_type = std::mem::zeroed();
        let mut hw_index = std::mem::zeroed();
        let mut hw_channel = std::mem::zeroed();

        let status = xl::xlGetApplConfig(
            app_name.as_ptr() as *mut i8,
            app_channel,
            &mut hw_type,
            &mut hw_index,
            &mut hw_channel,
            xl::XL_BUS_TYPE_CAN,
        );
        match status as u32 {
            xl::XL_SUCCESS => Ok(ChannelConfig {
                hw_type: HwType::from_repr(hw_type).unwrap(),
                hw_index,
                hw_channel,
            }),
            _ => Err(Error::DriverError(format!("xlGetApplConfig failed, err {}", status)).into()),
        }
    }
}

#[allow(dead_code)]
pub fn xl_get_channel_index(app_config: &ChannelConfig) -> Result<u32> {
    unsafe {
        Ok(xl::xlGetChannelIndex(
            app_config.hw_type as i32,
            app_config.hw_index as i32,
            app_config.hw_channel as i32,
        ) as u32)
    }
}

pub fn xl_get_channel_mask(app_config: &ChannelConfig) -> Result<XLaccess> {
    unsafe {
        Ok(xl::xlGetChannelMask(
            app_config.hw_type as i32,
            app_config.hw_index as i32,
            app_config.hw_channel as i32,
        ))
    }
}

pub fn xl_open_port(user_name: &str, access_mask: XLaccess, init: bool) -> Result<PortHandle> {
    unsafe {
        let mut port_handle = std::mem::zeroed();
        let mut permission_mask = std::mem::zeroed();
        if init {
            permission_mask = access_mask; // Request init access so we can change bitrate
        }

        let status = xl::xlOpenPort(
            &mut port_handle,
            user_name.as_ptr() as *mut i8,
            access_mask,
            &mut permission_mask,
            DEFAULT_RX_QUEUE_SIZE,
            xl::XL_INTERFACE_VERSION_V4,
            xl::XL_BUS_TYPE_CAN,
        );

        match status as u32 {
            xl::XL_SUCCESS => Ok(PortHandle {
                port_handle,
                permission_mask,
            }),
            _ => Err(Error::DriverError(format!("xlOpenPort failed, err {}", status)).into()),
        }
    }
}

pub fn xl_close_port(port_handle: &PortHandle) -> Result<()> {
    unsafe {
        let status = xl::xlClosePort(port_handle.port_handle);
        match status as u32 {
            xl::XL_SUCCESS => Ok(()),
            _ => Err(Error::DriverError(format!("xlClosePort failed, err {}", status)).into()),
        }
    }
}

pub fn xl_activate_channel(port_handle: &PortHandle, access_mask: XLaccess) -> Result<()> {
    unsafe {
        let status =
            xl::xlActivateChannel(port_handle.port_handle, access_mask, xl::XL_BUS_TYPE_CAN, 0);
        match status as u32 {
            xl::XL_SUCCESS => Ok(()),
            _ => {
                Err(Error::DriverError(format!("xlActivateChannel failed, err {}", status)).into())
            }
        }
    }
}

pub fn xl_deactivate_channel(port_handle: &PortHandle, access_mask: XLaccess) -> Result<()> {
    unsafe {
        let status = xl::xlDeactivateChannel(port_handle.port_handle, access_mask);
        match status as u32 {
            xl::XL_SUCCESS => Ok(()),
            _ => Err(
                Error::DriverError(format!("xlDeactivateChannel failed, err {}", status)).into(),
            ),
        }
    }
}

pub fn xl_can_fd_set_configuration(
    port_handle: &PortHandle,
    access_mask: XLaccess,
    config: &XLcanFdConf,
) -> Result<()> {
    unsafe {
        let mut config = config.clone();
        let status = xl::xlCanFdSetConfiguration(port_handle.port_handle, access_mask, &mut config);
        match status as u32 {
            xl::XL_SUCCESS => Ok(()),
            _ => Err(
                Error::DriverError(format!("xlCanFdSetConfiguration failed, err {}", status))
                    .into(),
            ),
        }
    }
}

pub fn xl_can_transmit_ex(
    port_handle: &PortHandle,
    access_mask: XLaccess,
    events: &[XLcanTxEvent],
) -> Result<u32> {
    unsafe {
        let msg_cnt = events.len() as u32;
        let mut msg_cnt_sent = std::mem::zeroed();

        // Clone so we can pass a mut ptr to the C API
        let mut boxed = events.to_owned();

        let status = xl::xlCanTransmitEx(
            port_handle.port_handle,
            access_mask,
            msg_cnt,
            &mut msg_cnt_sent,
            boxed.as_mut_ptr(),
        );

        match status as u32 {
            xl::XL_SUCCESS => Ok(msg_cnt_sent),
            _ => Err(Error::DriverError(format!("xlCanTransmitEx failed, err {}", status)).into()),
        }
    }
}

pub fn xl_can_receive(port_handle: &PortHandle) -> Result<Option<XLcanRxEvent>> {
    unsafe {
        let mut event: XLcanRxEvent = ::std::mem::zeroed();
        let status = xl::xlCanReceive(port_handle.port_handle, &mut event);
        match status as u32 {
            xl::XL_SUCCESS => {
                // tracing::info!("Got events size: {}, tag 0x{:x}", event.size, event.tag);
                Ok(Some(event))
            }
            xl::XL_ERR_QUEUE_IS_EMPTY => Ok(None),
            _ => Err(Error::DriverError(format!("xlCanReceive failed, err {}", status)).into()),
        }
    }
}
