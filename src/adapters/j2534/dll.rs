//! DLL discovery and architecture validation utilities.
use super::error::Error as J2534Error;
use crate::Result;

#[derive(Debug, PartialEq, Eq)]
enum DllMachine {
    X86,
    X64,
    Arm64,
    Other(u16),
}

/// Resolve the PassThru DLL path.
///
/// If `dll_path` is `Some`, uses it directly (after architecture check).
/// If `None`, discovers the first 64-bit driver from the Windows registry.
pub fn resolve_dll_path(dll_path: Option<&str>) -> Result<String> {
    let path = if let Some(p) = dll_path {
        p.to_owned()
    } else {
        let (native, wow32) = enumerate_passthru_drivers();

        if let Some(p) = native.into_iter().next() {
            p
        } else if wow32.is_empty() {
            return Err(J2534Error::DllError(
                "No J2534 PassThru drivers found in \
                 HKLM\\SOFTWARE\\PassThruSupport.04.04"
                    .to_owned(),
            )
            .into());
        } else {
            return Err(J2534Error::DllError(format!(
                "No 64-bit J2534 drivers found. \
                 The following device(s) have 32-bit-only drivers registered \
                 under HKLM\\SOFTWARE\\WOW6432Node\\PassThruSupport.04.04, \
                 which cannot be loaded by this 64-bit process:\n  {}\n\
                 Options:\n  \
                   1. Install 64-bit drivers for your device (check manufacturer's website).\n  \
                   2. Use `j2534:<path>` to specify a 64-bit DLL explicitly.\n  \
                   3. Use a 32-bit build instead.",
                wow32.join("\n  ")
            ))
            .into());
        }
    };

    check_dll_architecture(&path)?;
    Ok(path)
}

/// Returns `(native_64bit_paths, wow32_paths)`.
fn enumerate_passthru_drivers() -> (Vec<String>, Vec<String>) {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    const PASSTHRU_KEY: &str = "SOFTWARE\\PassThruSupport.04.04";
    const PASSTHRU_KEY_WOW: &str = "SOFTWARE\\WOW6432Node\\PassThruSupport.04.04";

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    let native = read_passthru_paths(&hklm, PASSTHRU_KEY).unwrap_or_default();
    let wow32 = read_passthru_paths(&hklm, PASSTHRU_KEY_WOW)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !native.contains(p))
        .collect();

    (native, wow32)
}

fn read_passthru_paths(hklm: &winreg::RegKey, key: &str) -> Result<Vec<String>> {
    use winreg::enums::KEY_READ;

    let root = hklm
        .open_subkey_with_flags(key, KEY_READ)
        .map_err(|e| J2534Error::DllError(e.to_string()))?;

    let paths = root
        .enum_keys()
        .flatten()
        .filter_map(|name| {
            root.open_subkey_with_flags(&name, KEY_READ)
                .ok()
                .and_then(|sub| sub.get_value::<String, _>("FunctionLibrary").ok())
        })
        .collect();
    Ok(paths)
}

fn dll_machine(path: &str) -> std::io::Result<DllMachine> {
    use std::io::{Read, Seek, SeekFrom};

    let mut f = std::fs::File::open(path)?;

    let mut magic = [0u8; 2];
    f.read_exact(&mut magic)?;
    if magic != [b'M', b'Z'] {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a PE file (no MZ header)",
        ));
    }

    f.seek(SeekFrom::Start(0x3C))?;
    let mut pe_offset_bytes = [0u8; 4];
    f.read_exact(&mut pe_offset_bytes)?;
    let pe_offset = u32::from_le_bytes(pe_offset_bytes) as u64;

    f.seek(SeekFrom::Start(pe_offset))?;
    let mut sig = [0u8; 4];
    f.read_exact(&mut sig)?;
    if sig != [b'P', b'E', 0, 0] {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a valid PE file (bad PE signature)",
        ));
    }

    let mut machine_bytes = [0u8; 2];
    f.read_exact(&mut machine_bytes)?;
    let machine = u16::from_le_bytes(machine_bytes);

    Ok(match machine {
        0x014C => DllMachine::X86,
        0x8664 => DllMachine::X64,
        0xAA64 => DllMachine::Arm64,
        other => DllMachine::Other(other),
    })
}

fn check_dll_architecture(path: &str) -> Result<()> {
    let machine = match dll_machine(path) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };

    #[cfg(target_arch = "x86_64")]
    if machine == DllMachine::X86 {
        return Err(J2534Error::DllError(format!(
            "J2534 DLL '{path}' is 32-bit (IMAGE_FILE_MACHINE_I386) and cannot \
             be loaded by this 64-bit process.\n\
             Options:\n  \
               1. Install 64-bit drivers for your device.\n  \
               2. Use a 32-bit build instead."
        ))
        .into());
    }

    #[cfg(target_arch = "x86")]
    if machine == DllMachine::X64 {
        return Err(J2534Error::DllError(format!(
            "J2534 DLL '{path}' is 64-bit (IMAGE_FILE_MACHINE_AMD64) and cannot \
             be loaded by this 32-bit process.\n\
             Options:\n  \
               1. Install 32-bit drivers for your device.\n  \
               2. Use a 64-bit build instead."
        ))
        .into());
    }

    Ok(())
}
