// SPDX-License-Identifier: MIT
//! Pause frame settings and counters, the equivalent of `ethtool -a` and
//! `ethtool -I -a`.
//!
//! Run with no flag to see what a request *without* `ETHTOOL_FLAG_STATS`
//! returns, and with `--stats` to see what the flag adds.
use netlink_packet_ethtool::{EthtoolCmd, EthtoolMessage, PauseAttr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args().nth(1).unwrap();
    let with_stats = std::env::args().any(|a| a == "--stats");

    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    let attrs = if with_stats {
        PauseAttr::request_with_stats(&dev)
    } else {
        PauseAttr::request(&dev)
    };

    println!(
        "Pause parameters for {dev}:  (ETHTOOL_FLAG_STATS {})",
        if with_stats { "set" } else { "NOT set" }
    );
    let mut saw_stats = false;
    for reply in handle
        .request(EthtoolMessage::Pause(EthtoolCmd::PauseGet, attrs), false)
        .await?
    {
        let EthtoolMessage::Pause(_, attributes) = reply else {
            continue;
        };
        for attribute in &attributes {
            match attribute {
                PauseAttr::Autoneg(on) => {
                    println!("Autonegotiate:\t{}", if *on { "on" } else { "off" })
                }
                PauseAttr::Rx(on) => {
                    println!("RX:\t\t{}", if *on { "on" } else { "off" })
                }
                PauseAttr::Tx(on) => {
                    println!("TX:\t\t{}", if *on { "on" } else { "off" })
                }
                PauseAttr::Stats(stats) => {
                    saw_stats = true;
                    println!("Statistics:");
                    if let Some(v) = stats.tx_frames {
                        println!("  tx_pause_frames: {v}");
                    }
                    if let Some(v) = stats.rx_frames {
                        println!("  rx_pause_frames: {v}");
                    }
                    if let Some(v) = stats.tx_pause_storm_events {
                        println!("  tx_pause_storm_events: {v}");
                    }
                }
                _ => {}
            }
        }
    }
    if !saw_stats {
        println!("(no statistics in the reply)");
    }
    Ok(())
}
