use crate::{
    xlActivateChannel, xlCanFdSetConfiguration, xlCanSetChannelBitrate, xlCanSetChannelMode, xlCanSetChannelParamsC200,
    xlCloseDriver, xlClosePort, xlDeactivateChannel, xlGetApplConfig, xlGetChannelIndex, xlGetDriverConfig,
    xlOpenDriver, xlOpenPort, xlSetNotification, xlCanTransmit, xlCanTransmitEx, xlReceive, XLevent,  XLcanTxEvent, XLaccess, XLcanFdConf, XLchannelConfig, XLdriverConfig, XLhandle,
    XLportHandle, XL_BUS_TYPE_CAN, XL_SUCCESS, XL_ERR_QUEUE_IS_EMPTY,
};

use crate::vector::types::{ApplicationConfig, PortConfig};

use crate::vector::bit_timing::BitTiming;

use crate::vector::error::Error;

use super::bit_timing::BitTimingFd;

pub fn open_driver() -> Result<(), Error> {
    let status = unsafe { xlOpenDriver() };

    match status as u32 {
        XL_SUCCESS => Ok(()),
        _ => Err(Error::DriverError(format!(
            "Failed to open driver with error code: {}",
            status
        ))),
    }
}

pub fn close_driver() -> Result<(), Error> {
    let status = unsafe { xlCloseDriver() };
    match status as u32 {
        XL_SUCCESS => Ok(()),

        _ => Err(Error::DriverError(format!(
            "Failed to close driver with error code: {}",
            status
        ))),
    }
}

pub fn open_port(
    app_name: &str,
    mask: XLaccess,
    permission_mask: Option<XLaccess>,
    rx_queue_size: u32,
    interface_version: u32,
    bus_type: u32,
) -> Result<PortConfig, Error> {
    let mut port_handle: XLportHandle = 0;
    let mut permission_mask = match permission_mask {
        Some(mask) => mask,
        None => 0,
    };

    let status = unsafe {
        xlOpenPort(
            &mut port_handle,
            app_name.as_ptr() as *mut i8,
            mask,
            &mut permission_mask,
            rx_queue_size,
            interface_version,
            bus_type,
        )
    };

    match status as u32 {
        XL_SUCCESS => Ok(PortConfig {
            port_handle,
            permission_mask: permission_mask as XLaccess,
        }),

        _ => Err(Error::DriverError(format!(
            "Failed to open port with error code: {}",
            status
        ))),
    }
}

pub fn close_port(port_handle: XLportHandle) -> Result<(), Error> {
    let status = unsafe { xlClosePort(port_handle) };

    match status as u32 {
        XL_SUCCESS => Ok(()),
        _ => Err(Error::DriverError(format!(
            "Failed to close port with error code: {}",
            status
        ))),
    }
}

pub fn send_can(
    port_handle: XLportHandle,
    access_mask: XLaccess,
    events_count: u32,
    events: Vec<XLevent>
) -> Result<u32, Error> {
    unsafe {
        let mut count = events_count.clone();
        // let mut events_clone = events.clone();
        // println!("Events address: {:p}", &events_clone);
        // println!("Count address: {:p}", &count);
        let mut boxed = events.clone().into_boxed_slice();
        let mut array = boxed.as_mut_ptr();

        // println!("BBBBBB: {:?}", array);
        let status = xlCanTransmit(
            port_handle,
            access_mask,
            &mut count as *mut u32,
            array as *mut _ as *mut std::os::raw::c_void,
        );

        match status as u32 {
            XL_SUCCESS => (),
            _ => {
                return Err(Error::DriverError(format!(
                    "Failed to send data to CAN with error: {}",
                    status
                )))
            }
        };

        Ok(count)
    }
}

pub fn send_can_fd(
    port_handle: XLportHandle,
    access_mask: XLaccess,
    events_count: usize,
    events: Vec<XLcanTxEvent>
) {
    
}

pub fn receive_can(
    port_handle: XLportHandle,
) -> Result<XLevent, Error> {
    unsafe {
        let mut event: XLevent = std::mem::zeroed(); //XLcanFdConf {
        let mut out_count = 1u32;
        let status = xlReceive(
            port_handle,
            &mut out_count as *mut u32,
            &mut event as *mut XLevent
        );

        match status as u32 {
            XL_SUCCESS => (),
            XL_ERR_QUEUE_IS_EMPTY => return Err(Error::EmptyQueue),
            _ => {
                return Err(Error::DriverError(format!(
                    "Failed to receive data from CAN with error: {}",
                    status
                )))
            }
        };

        Ok(event)
    }
}

pub fn set_bit_timing(
    port_handle: XLportHandle,
    mut channel_mask: u64,
    permission_mask: u64,
    timing: &BitTiming,
) -> Result<(), Error> {
    channel_mask = channel_mask & permission_mask;

    if channel_mask == 0 {
        return Ok(());
    }

    let status = unsafe { xlCanSetChannelParamsC200(port_handle, channel_mask, timing.btr0(), timing.btr1()) };

    match status as u32 {
        XL_SUCCESS => (),
        _ => {
            return Err(Error::DriverError(format!(
                "Failed to set bit timing with error code: {}",
                status
            )))
        }
    };

    Ok(())
}

pub fn set_bit_timing_fd(
    port_handle: XLportHandle,
    mut channel_mask: u64,
    permission_mask: u64,
    timing: &BitTimingFd,
) -> Result<(), Error> {
    channel_mask = channel_mask & permission_mask;

    if channel_mask == 0 {
        return Ok(());
    }

    unsafe {
        let mut conf: XLcanFdConf = std::mem::zeroed(); //XLcanFdConf {
        conf.arbitrationBitRate = timing.nom_bitrate();
        conf.sjwAbr = timing.nom_sjw;
        conf.tseg1Abr = timing.nom_tseg1;
        conf.tseg2Abr = timing.nom_tseg2;
        conf.dataBitRate = timing.data_bitrate();
        conf.sjwDbr = timing.data_sjw;
        conf.tseg1Dbr = timing.data_tseg1;
        conf.tseg2Dbr = timing.data_tseg2;

        let status = xlCanFdSetConfiguration(port_handle, channel_mask, &mut conf as *mut XLcanFdConf);

        match status as u32 {
            XL_SUCCESS => (),
            _ => {
                return Err(Error::DriverError(format!(
                    "Failed to set bit timing with error code: {}",
                    status
                )))
            }
        };
    };

    Ok(())
}

pub fn set_bit_rate(
    port_handle: XLportHandle,
    mut channel_mask: u64,
    permission_mask: u64,
    bit_rate: u32,
) -> Result<(), Error> {
    channel_mask = channel_mask & permission_mask;
    if channel_mask == 0 {
        return Ok(());
    }

    let status = unsafe { xlCanSetChannelBitrate(port_handle, channel_mask, bit_rate) };
    match status as u32 {
        XL_SUCCESS => (),
        _ => {
            return Err(Error::DriverError(format!(
                "Failed to set bit rate with error code: {}",
                status
            )))
        }
    };

    // let mut timing = BitTiming::new(bit_rate)?;

    // set_bit_timing(port_handle, channel_mask, permission_mask, &timing)
    //todo!()
    Ok(())
}

pub fn set_channel_mode(port_handle: XLportHandle, channel_mask: XLaccess, tx: i32, txrq: i32) -> Result<(), Error> {
    let status = unsafe { xlCanSetChannelMode(port_handle, channel_mask, tx, txrq) };

    match status as u32 {
        XL_SUCCESS => Ok(()),
        _ => Err(Error::DriverError(format!(
            "Failed to set channel mode with error code: {}",
            status
        ))),
    }
}

pub fn set_notification(port_handle: XLportHandle, event_handle: &mut XLhandle, queue_level: i32) -> Result<(), Error> {
    let status = unsafe { xlSetNotification(port_handle, event_handle, queue_level) };

    match status as u32 {
        XL_SUCCESS => Ok(()),
        _ => Err(Error::DriverError(format!(
            "Failed to set notification with error code: {}",
            status
        ))),
    }
}

pub fn activate_channel(
    port_handle: XLportHandle,
    channel_mask: XLaccess,
    bus_type: u32,
    flags: u32,
) -> Result<(), Error> {
    let status = unsafe { xlActivateChannel(port_handle, channel_mask, bus_type, flags) };

    match status as u32 {
        XL_SUCCESS => Ok(()),
        _ => Err(Error::DriverError(format!(
            "Failed to activate channel with error code: {}",
            status
        ))),
    }
}

pub fn deactivate_channel(port_handle: XLportHandle, channel_mask: XLaccess) -> Result<(), Error> {
    let status = unsafe { xlDeactivateChannel(port_handle, channel_mask) };

    match status as u32 {
        XL_SUCCESS => Ok(()),
        _ => Err(Error::DriverError(format!(
            "Failed to deactivate channel with error code: {}",
            status
        ))),
    }
}

pub fn find_global_channel_idx(
    channel: u8,

    serial: Option<u32>,

    app_name: Option<&str>,

    channel_configs: Vec<XLchannelConfig>,
) -> Result<u8, Error> {
    if let Some(serial) = serial {
        let mut serial_found = false;

        for channel_config in channel_configs {
            if channel_config.serialNumber == serial {
                continue;
            }

            serial_found = true;

            if channel_config.hwChannel == channel {
                return Ok(channel_config.channelIndex);
            }
        }

        match serial_found {
            true => {
                return Err(Error::DriverError(format!(
                    "Channel {} not found on interface with serial: {}",
                    channel, serial
                )))
            }

            false => return Err(Error::DriverError(format!("No interface with serial {} found", serial))),
        };
    }

    if let Some(app_name) = app_name {
        let app_config = get_application_config(app_name, channel as u32)?;
        let idx = unsafe { xlGetChannelIndex(app_config.hw_type, app_config.hw_index, app_config.hw_channel) };

        if idx < 0 {
            // Undocumented behavior! See issue #353.
            // If hardware is unavailable, this function returns -1.
            // Raise an exception as if the driver
            // would have signalled XL_ERR_HW_NOT_PRESENT.
            return Err(Error::DriverError(format!(
                "Failed to get channel id, due to undocumented behavior"
            )));
        }

        return Ok(idx as u8);
    }

    for channel_config in channel_configs {
        if channel_config.hwChannel == channel {
            return Ok(channel_config.channelIndex);
        }
    }

    Err(Error::DriverError(format!("Channel {} not found", channel)))
}

pub fn get_channel_configs() -> Result<Vec<XLchannelConfig>, Error> {
    let driver_config = get_driver_config()?;
    let mut channels: Vec<XLchannelConfig> = Vec::new();

    for i in 0..driver_config.channelCount {
        channels.push(driver_config.channel[i as usize]);

        // let mut channel_config: XLchannelConfig = std::mem::zeroed();
        // let status = unsafe { xlGetChannelConfig(i, &mut channel_config) };
        // match status as u32 {
        //     XL_SUCCESS => channels.push(channel_config),
        //     _ => return Err(Error::DriverError(format!("Failed to get channel config with error code: {}", status)))
        // }
    }

    Ok(channels)
}

pub fn get_driver_config() -> Result<XLdriverConfig, Error> {
    unsafe {
        let mut driver_config: XLdriverConfig = std::mem::zeroed();

        let status = xlGetDriverConfig(&mut driver_config);
        match status as u32 {
            XL_SUCCESS => Ok(driver_config),

            _ => Err(Error::DriverError(format!(
                "Failed to get driver config with error code: {}",
                status
            ))),
        }
    }
}

pub fn get_application_config(app_name: &str, app_channel: u32) -> Result<ApplicationConfig, Error> {
    unsafe {
        let mut hw_type = std::mem::zeroed();
        let mut hw_index = std::mem::zeroed();
        let mut hw_channel = std::mem::zeroed();

        let status = xlGetApplConfig(
            app_name.as_ptr() as *mut i8,
            app_channel,
            &mut hw_type,
            &mut hw_index,
            &mut hw_channel,
            XL_BUS_TYPE_CAN,
        );

        match status as u32 {
            XL_SUCCESS => Ok(ApplicationConfig {
                hw_type: hw_type as i32,
                hw_index: hw_index as i32,
                hw_channel: hw_channel as i32,
            }),

            _ => Err(Error::DriverError(format!(
                "Failed to get application config with error code: {}",
                status
            ))),
        }
    }
}
