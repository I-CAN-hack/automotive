use std::ffi::CStr;
use std::sync::LazyLock;

use crate::adapters::vector::bindings as xl;
use crate::adapters::vector::error::Error;
use crate::adapters::vector::types::{
    ChannelConfig, HwType, PortHandle, XLaccess, XLcanFdConf, XLcanRxEvent, XLcanTxEvent,
};
use crate::Result;

// CAN FD: Size of the port receive queue allocated by the driver in bytes. The value must be a
// power of 2 and within a range of 8192…524288 bytes (0.5 MB).
const DEFAULT_RX_QUEUE_SIZE: u32 = 52428;

static XL: LazyLock<Result<xl::Xl>> = LazyLock::new(|| {
    let filename = libloading::library_filename("vxlapi64");
    Ok(unsafe { xl::Xl::new(filename) }?)
});

fn vxlapi64() -> Result<&'static xl::Xl> {
    XL.as_ref().map_err(|e| e.clone())
}

fn xl_status_description(status: i32) -> String {
    unsafe {
        match vxlapi64() {
            Ok(api) => {
                let message = api.xlGetErrorString(status);
                if message.is_null() {
                    format!("error code {status}")
                } else {
                    CStr::from_ptr(message).to_string_lossy().into_owned()
                }
            }
            Err(_) => format!("error code {status}"),
        }
    }
}

fn driver_error(operation: &str, status: i32) -> Error {
    Error::DriverError(format!(
        "{operation} failed, {}",
        xl_status_description(status)
    ))
}

pub fn xl_open_driver() -> Result<()> {
    let status = unsafe { vxlapi64()?.xlOpenDriver() };

    match status as u32 {
        xl::XL_SUCCESS => Ok(()),
        _ => Err(driver_error("xlOpenDriver", status).into()),
    }
}

pub fn xl_close_driver() -> Result<()> {
    let status = unsafe { vxlapi64()?.xlCloseDriver() };

    match status as u32 {
        xl::XL_SUCCESS => Ok(()),
        _ => Err(driver_error("xlCloseDriver", status).into()),
    }
}

pub fn xl_get_driver_config(channel_idx: usize) -> Result<ChannelConfig> {
    unsafe {
        let mut config: xl::XLdriverConfig = std::mem::zeroed();
        let status = vxlapi64()?.xlGetDriverConfig(&mut config);

        match status as u32 {
            xl::XL_SUCCESS => {
                let channel_count: usize = config.channelCount as usize;
                tracing::info!("Channel count {}", channel_count);

                if channel_idx >= channel_count {
                    return Err(Error::DriverError(format!(
                        "Requested channel index {} but only {} channels available",
                        channel_idx, channel_count
                    ))
                    .into());
                }

                let channel = config.channel[channel_idx];

                Ok(ChannelConfig {
                    hw_type: HwType::from_repr(channel.hwType as u32).unwrap(),
                    hw_index: channel.hwIndex as u32,
                    hw_channel: channel.hwChannel as u32,
                })
            }
            _ => Err(driver_error("xlGetDriverConfig", status).into()),
        }
    }
}

#[allow(dead_code)]
pub fn xl_get_application_config(app_name: &str, app_channel: u32) -> Result<ChannelConfig> {
    unsafe {
        let mut hw_type = std::mem::zeroed();
        let mut hw_index = std::mem::zeroed();
        let mut hw_channel = std::mem::zeroed();

        let status = vxlapi64()?.xlGetApplConfig(
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
            _ => Err(driver_error("xlGetApplConfig", status).into()),
        }
    }
}

#[allow(dead_code)]
pub fn xl_get_channel_index(app_config: &ChannelConfig) -> Result<u32> {
    unsafe {
        Ok(vxlapi64()?.xlGetChannelIndex(
            app_config.hw_type as i32,
            app_config.hw_index as i32,
            app_config.hw_channel as i32,
        ) as u32)
    }
}

pub fn xl_get_channel_mask(app_config: &ChannelConfig) -> Result<XLaccess> {
    unsafe {
        Ok(vxlapi64()?.xlGetChannelMask(
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

        let status = vxlapi64()?.xlOpenPort(
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
            _ => Err(driver_error("xlOpenPort", status).into()),
        }
    }
}

pub fn xl_close_port(port_handle: &PortHandle) -> Result<()> {
    unsafe {
        let status = vxlapi64()?.xlClosePort(port_handle.port_handle);
        match status as u32 {
            xl::XL_SUCCESS => Ok(()),
            _ => Err(driver_error("xlClosePort", status).into()),
        }
    }
}

pub fn xl_activate_channel(port_handle: &PortHandle, access_mask: XLaccess) -> Result<()> {
    unsafe {
        let status = vxlapi64()?.xlActivateChannel(
            port_handle.port_handle,
            access_mask,
            xl::XL_BUS_TYPE_CAN,
            0,
        );
        match status as u32 {
            xl::XL_SUCCESS => Ok(()),
            _ => Err(driver_error("xlActivateChannel", status).into()),
        }
    }
}

pub fn xl_deactivate_channel(port_handle: &PortHandle, access_mask: XLaccess) -> Result<()> {
    unsafe {
        let status = vxlapi64()?.xlDeactivateChannel(port_handle.port_handle, access_mask);
        match status as u32 {
            xl::XL_SUCCESS => Ok(()),
            _ => Err(driver_error("xlDeactivateChannel", status).into()),
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
        let status =
            vxlapi64()?.xlCanFdSetConfiguration(port_handle.port_handle, access_mask, &mut config);
        match status as u32 {
            xl::XL_SUCCESS => Ok(()),
            _ => Err(driver_error("xlCanFdSetConfiguration", status).into()),
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

        let status = vxlapi64()?.xlCanTransmitEx(
            port_handle.port_handle,
            access_mask,
            msg_cnt,
            &mut msg_cnt_sent,
            boxed.as_mut_ptr(),
        );

        match status as u32 {
            xl::XL_SUCCESS | xl::XL_ERR_QUEUE_IS_FULL => Ok(msg_cnt_sent),
            _ => Err(driver_error("xlCanTransmitEx", status).into()),
        }
    }
}

pub fn xl_can_receive(port_handle: &PortHandle) -> Result<Option<XLcanRxEvent>> {
    unsafe {
        let mut event: XLcanRxEvent = ::std::mem::zeroed();
        let status = vxlapi64()?.xlCanReceive(port_handle.port_handle, &mut event);
        match status as u32 {
            xl::XL_SUCCESS => {
                // tracing::info!("Got events size: {}, tag 0x{:x}", event.size, event.tag);
                Ok(Some(event))
            }
            xl::XL_ERR_QUEUE_IS_EMPTY => Ok(None),
            _ => Err(driver_error("xlCanReceive", status).into()),
        }
    }
}
