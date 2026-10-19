// SPDX-License-Identifier: MIT
//! Standardised statistics with their names resolved, the equivalent of
//! `ethtool -S <dev> --groups eth-phy eth-mac eth-ctrl rmon`.
use ethtool2::STANDARD_STATS_GROUPS;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args().nth(1).ok_or("usage: stats <dev>")?;
    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    let stats = handle.stats(&dev, &STANDARD_STATS_GROUPS).await?;

    println!("Standard stats for {dev}:");
    for group in &stats.groups {
        let label = group.label().unwrap_or("?");
        for counter in &group.counters {
            println!(
                "{label}-{}: {}",
                counter.name.as_deref().unwrap_or("<unnamed>"),
                counter.value
            );
        }
        for (which, buckets) in [("rx", &group.hist_rx), ("tx", &group.hist_tx)] {
            for bucket in buckets {
                // ethtool(8) names the first bucket by its upper bound
                // alone -- "Pkts64Octets", not "Pkts0to64Octets".
                let range = match (bucket.low, bucket.high) {
                    (Some(0), Some(high)) => format!("{high}"),
                    (Some(low), Some(high)) => format!("{low}to{high}"),
                    (Some(low), None) => format!("{low}toMax"),
                    _ => continue,
                };
                println!(
                    "{which}-{label}-etherStatsPkts{range}Octets: {}",
                    bucket.value.unwrap_or(0)
                );
            }
        }
    }
    Ok(())
}
