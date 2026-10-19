// SPDX-License-Identifier: MIT
//! Walk a reply generically, printing nla kind/len, to settle encodings.
//!
//! `probe_raw <dev> [depth] [group...]` sends a `STATS_GET` through the raw
//! escape hatch, so nothing on the way back is decoded by the packet crate.
use ethtool2::{
    packet_ethtool::{constants::ETHTOOL_MSG_STATS_GET, StatsAttr},
    Bitset, Device, RawMessage,
};
use netlink_packet_core::NlasIterator;

fn walk(payload: &[u8], depth: usize, limit: usize) {
    if depth > limit {
        return;
    }
    let pad = "  ".repeat(depth);
    for nla in NlasIterator::new(payload) {
        match nla {
            Ok(nla) => {
                let kind = nla.kind();
                let len = nla.value().len();
                println!("{pad}kind {:<5} (0x{:04x}) len {len}", kind & 0x3fff, kind);
                // Replies do NOT set NLA_F_NESTED, so descend on anything
                // that parses as a nest rather than trusting the flag.
                if depth < limit && len >= 4 {
                    let looks_nested = NlasIterator::new(nla.value()).all(|n| n.is_ok())
                        && NlasIterator::new(nla.value()).count() > 0;
                    if looks_nested {
                        walk(nla.value(), depth + 1, limit);
                    } else {
                        println!("{pad}  = {:02x?}", &nla.value()[..len.min(16)]);
                    }
                }
            }
            Err(e) => {
                println!("{pad}<iteration error: {e}>");
                return;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args()
        .nth(1)
        .ok_or("usage: probe_raw <dev> [depth] [group...]")?;
    let depth: usize = std::env::args()
        .nth(2)
        .and_then(|a| a.parse().ok())
        .unwrap_or(3);
    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    let groups: Vec<String> = std::env::args().skip(3).collect();
    let groups = if groups.is_empty() {
        vec!["eth-mac".to_string()]
    } else {
        groups
    };
    let request = RawMessage::encode(ETHTOOL_MSG_STATS_GET, &stats_by_name(&dev, groups));

    match handle.raw(request, false).await {
        Ok(replies) => {
            println!("{} reply message(s)", replies.len());
            for reply in &replies {
                println!(
                    "reply cmd {} ({} payload bytes)",
                    reply.cmd,
                    reply.payload.len()
                );
                walk(&reply.payload, 0, depth);
            }
        }
        Err(e) => println!("request failed: {e}"),
    }
    Ok(())
}

/// A `STATS_GET` selecting `groups` by whatever names were typed, known to
/// the packet crate or not, so the probe can ask for groups it cannot model.
fn stats_by_name(dev: &str, groups: Vec<String>) -> Vec<StatsAttr> {
    vec![
        StatsAttr::Header(Device::from(dev).header(Default::default())),
        StatsAttr::Groups(Bitset::select_names(groups)),
    ]
}
