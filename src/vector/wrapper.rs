use crate::{xlOpenDriver, XL_SUCCESS};

use crate::vector::error::Error;

pub fn open_driver() -> Result<(), Error> {
    let status = unsafe { xlOpenDriver() };
    match status as u32 {
        XL_SUCCESS => Ok(()),
        // _ => Err(Error::DriverError(format!("Failed to open driver with error code: {}", status)))
        _ => Err(Error::DriverError(1))
    }
}

// pub fn close_driver() -> Result<(), Error> {
//     
// }
