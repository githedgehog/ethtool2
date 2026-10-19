// SPDX-License-Identifier: MIT
//! Half a page of module memory, the equivalent of
//! `ethtool -m <dev> page <page> bank <bank> offset <offset> length 128 raw on`.
//!
//! `module_eeprom <dev|ifindex> [page] [bank] [lower]` writes the bytes to stdout,
//! so the output can be compared with ethtool's byte for byte.
use std::io::Write;

use netlink_packet_ethtool::{EepromWindow, EthtoolCmd, EthtoolMessage, ModuleEepromAttr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let dev = args.get(1).ok_or("usage: module_eeprom <dev|ifindex> [page] [bank] [lower]")?;
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
    let attrs = match dev.parse::<u32>() {
        Ok(index) => ModuleEepromAttr::request_by_index(index, window),
        Err(_) => ModuleEepromAttr::request(dev, window),
    };

    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);
    for reply in handle
        .request(
            EthtoolMessage::ModuleEeprom(EthtoolCmd::ModuleEepromGet, attrs),
            false,
        )
        .await?
    {
        let EthtoolMessage::ModuleEeprom(_, attributes) = reply else {
            continue;
        };
        let data = ModuleEepromAttr::data(&attributes).ok_or("reply without data")?;
        std::io::stdout().write_all(data)?;
    }
    Ok(())
}
