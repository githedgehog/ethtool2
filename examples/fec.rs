// SPDX-License-Identifier: MIT
//! FEC settings and counters, the equivalent of `ethtool --show-fec` and,
//! with `--stats`, `ethtool -I --show-fec`.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args()
        .nth(1)
        .ok_or("usage: fec <dev> [--stats]")?;
    let with_stats = std::env::args().any(|a| a == "--stats");
    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    let fec = if with_stats {
        handle.fec_with_stats(&dev).await?
    } else {
        handle.fec(&dev).await?
    };

    println!("FEC parameters for {dev}:");
    // ethtool(8) renders the `auto` flag as a mode called "Auto" in this
    // list, and prints "None" when nothing is configured.
    let mut modes = fec.configured();
    if fec.auto == Some(true) {
        modes.push("Auto");
    }
    if modes.is_empty() {
        modes.push("None");
    }
    println!("Supported/Configured FEC encodings: {}", modes.join(" "));
    // Absent means none active, which is what ethtool prints.
    println!(
        "Active FEC encoding: {}",
        match (fec.active, fec.active_name()) {
            (None, _) => "None".to_string(),
            (Some(_), Some(name)) => name.to_string(),
            (Some(bit), None) => format!("link-mode bit {bit}"),
        }
    );

    if let Some(stats) = &fec.stats {
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
            for (lane, value) in bin.per_lane.iter().enumerate() {
                println!("    lane {lane}: {value}");
            }
        }
    }
    Ok(())
}
