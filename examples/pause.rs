// SPDX-License-Identifier: MIT
//! Pause frame settings and counters, the equivalent of `ethtool -a` and
//! `ethtool -I -a`.
//!
//! Run with no flag to see what a request *without* `ETHTOOL_FLAG_STATS`
//! returns, and with `--stats` to see what the flag adds.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args()
        .nth(1)
        .ok_or("usage: pause <dev> [--stats]")?;
    let with_stats = std::env::args().any(|a| a == "--stats");

    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    let pause = if with_stats {
        handle.pause_with_stats(&dev).await?
    } else {
        handle.pause(&dev).await?
    };

    println!(
        "Pause parameters for {dev}:  (ETHTOOL_FLAG_STATS {})",
        if with_stats { "set" } else { "NOT set" }
    );
    let on_off = |v: bool| if v { "on" } else { "off" };
    if let Some(v) = pause.autoneg {
        println!("Autonegotiate:\t{}", on_off(v));
    }
    if let Some(v) = pause.rx {
        println!("RX:\t\t{}", on_off(v));
    }
    if let Some(v) = pause.tx {
        println!("TX:\t\t{}", on_off(v));
    }
    match pause.stats {
        Some(stats) => {
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
        None => println!("(no statistics in the reply)"),
    }
    Ok(())
}
