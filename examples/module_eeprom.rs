// SPDX-License-Identifier: MIT
//! Half a page of module memory, the equivalent of
//! `ethtool -m <dev> page <page> bank <bank> offset <offset> length 128 raw on`.
//!
//! `module_eeprom <dev|ifindex> [page] [bank] [lower]` writes the bytes to stdout,
//! so the output can be compared with ethtool's byte for byte. A device given
//! as a number is addressed by interface index.
//!
//! Needs `CAP_NET_ADMIN`.
use std::io::Write;

use ethtool2::{Device, EepromWindow};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let dev = args
        .get(1)
        .ok_or("usage: module_eeprom <dev|ifindex> [page] [bank] [lower]")?;
    let number = |at: usize| -> Result<u8, Box<dyn std::error::Error>> {
        match args.get(at) {
            Some(text) => Ok(u8::from_str_radix(text.trim_start_matches("0x"), 16)?),
            None => Ok(0),
        }
    };
    let (page, bank) = (number(2)?, number(3)?);
    let window = if args.iter().any(|a| a == "lower") {
        EepromWindow::lower(0x50)?
    } else {
        EepromWindow::upper(0x50, page, bank)?
    };
    let device = match dev.parse::<u32>() {
        Ok(index) => Device::Index(index),
        Err(_) => Device::from(dev),
    };

    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);
    let data = handle.module_eeprom(device, window).await?;
    std::io::stdout().write_all(&data)?;
    Ok(())
}
