// SPDX-License-Identifier: MIT
//! Standardised statistics with their names resolved, the equivalent of
//! `ethtool -S <dev> --groups eth-phy eth-mac eth-ctrl rmon`.
use netlink_packet_ethtool::{EthtoolCmd, EthtoolMessage, StatsAttr, StringSet, StrsetAttr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args().nth(1).unwrap();
    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    let replies = handle
        .request(
            EthtoolMessage::Stats(
                EthtoolCmd::StatsGet,
                StatsAttr::request(&dev, StatsAttr::STANDARD_GROUPS),
            ),
            false,
        )
        .await?;

    println!("Standard stats for {dev}:");
    for reply in &replies {
        let EthtoolMessage::Stats(_, attributes) = reply else {
            continue;
        };
        for attribute in attributes {
            let StatsAttr::Group(group) = attribute else {
                continue;
            };
            // Resolve this group's counter names from its own string set.
            let names = match group.string_set_id {
                Some(id) => fetch_names(&handle, &dev, id).await?,
                None => StringSet::default(),
            };
            let label = match group.group() {
                Some(g) => format!("{g:?}").to_lowercase(),
                None => "?".into(),
            };
            for stat in &group.stats {
                let name = names
                    .name(stat.index as u32)
                    .unwrap_or("<unnamed>")
                    .to_string();
                println!("{label}-{name}: {}", stat.value);
            }
            for (which, buckets) in [("rx", &group.hist_rx), ("tx", &group.hist_tx)] {
                for bucket in buckets {
                    // ethtool(8) names the first bucket by its upper bound
                    // alone -- "Pkts64Octets", not "Pkts0to64Octets".
                    let label = match (bucket.low, bucket.high) {
                        (Some(0), Some(high)) => format!("{high}"),
                        (Some(low), Some(high)) => format!("{low}to{high}"),
                        (Some(low), None) => format!("{low}toMax"),
                        _ => continue,
                    };
                    println!(
                        "{which}-rmon-etherStatsPkts{label}Octets: {}",
                        bucket.value.unwrap_or(0)
                    );
                }
            }
        }
    }
    Ok(())
}

async fn fetch_names(
    handle: &ethtool2::Handle,
    dev: &str,
    set_id: u32,
) -> Result<StringSet, Box<dyn std::error::Error>> {
    let replies = handle
        .request(
            EthtoolMessage::Strset(EthtoolCmd::StrsetGet, StrsetAttr::request(dev, set_id)),
            false,
        )
        .await?;
    for reply in replies {
        if let EthtoolMessage::Strset(_, attributes) = reply {
            for attribute in attributes {
                if let StrsetAttr::StringSets(sets) = attribute {
                    if let Some(set) = sets.into_iter().next() {
                        return Ok(set);
                    }
                }
            }
        }
    }
    Ok(StringSet::default())
}
