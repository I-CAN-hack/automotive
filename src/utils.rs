//! Utilities for displaying ECU memory.

/// Format bytes in 16-byte rows with addresses, hex bytes, and ASCII.
///
/// `address` is the ECU address displayed for the first byte.
/// `address_granularity` is bytes per ECU address, as negotiated by XCP.
/// The returned string has no trailing newline.
///
/// # Panics
///
/// Panics if `address_granularity` is zero.
pub fn hexdump(data: &[u8], address: u32, address_granularity: u8) -> String {
    assert!(
        address_granularity > 0,
        "address_granularity must be positive"
    );
    data.chunks(16)
        .enumerate()
        .map(|(row, chunk)| {
            let hex_data = chunk
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            let ascii_data: String = chunk
                .iter()
                .map(|&byte| {
                    if (32..127).contains(&byte) {
                        char::from(byte)
                    } else {
                        '.'
                    }
                })
                .collect();
            let address = address + (row * 16 / usize::from(address_granularity)) as u32;
            format!("{address:08x}  {hex_data:<47}  |{ascii_data}|")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
