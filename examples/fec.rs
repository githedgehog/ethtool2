// SPDX-License-Identifier: MIT
//! FEC settings and counters, the equivalent of `ethtool -I --show-fec`.
use netlink_packet_ethtool::{EthtoolCmd, EthtoolMessage, FecAttr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args().nth(1).unwrap();
    let with_stats = std::env::args().any(|a| a == "--stats");
    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    let attrs = if with_stats {
        FecAttr::request_with_stats(&dev)
    } else {
        FecAttr::request(&dev)
    };

    println!("FEC parameters for {dev}:");
    let mut modes: Vec<String> = Vec::new();
    let mut auto = false;
    let mut active: Option<u32> = None;
    let mut stats = None;
    for reply in handle
        .request(EthtoolMessage::Fec(EthtoolCmd::FecGet, attrs), false)
        .await?
    {
        let EthtoolMessage::Fec(_, attributes) = reply else {
            continue;
        };
        for attribute in attributes {
            match attribute {
                FecAttr::Modes(bitset) => {
                    modes = bitset.names().into_iter().map(str::to_string).collect()
                }
                FecAttr::Auto(on) => auto = on,
                FecAttr::Active(bit) => active = Some(bit),
                FecAttr::Stats(s) => stats = Some(s),
                _ => {}
            }
        }
    }

    // ethtool(8) renders the `auto` flag as a mode called "Auto" in this
    // list, and prints "None" when nothing is configured.
    if auto {
        modes.push("Auto".into());
    }
    if modes.is_empty() {
        modes.push("None".into());
    }
    println!("Supported/Configured FEC encodings: {}", modes.join(" "));
    // Absent means none active, which is what ethtool prints.
    println!(
        "Active FEC encoding: {}",
        match active {
            None | Some(0) => "None".to_string(),
            Some(bit) => format!("link-mode bit {bit}"),
        }
    );

    if let Some(stats) = stats {
        println!("Statistics:");
        for (label, counter) in [
            ("corrected_blocks", &stats.corrected),
            ("uncorrectable_blocks", &stats.uncorr),
            ("corrected_bits", &stats.corr_bits),
        ] {
            let Some(total) = counter.total else { continue };
            println!("  {label}: {total}");
            for (lane, value) in counter.lanes.iter().enumerate() {
                println!("    lane {lane}: {value}");
            }
        }
        for bin in &stats.hist {
            println!("  hist {:?}..{:?}: {:?}", bin.low, bin.high, bin.value);
        }
    }
    Ok(())
}
