// SPDX-License-Identifier: MIT
//! Walk a reply generically, printing nla kind/len, to settle encodings.
use netlink_packet_core::{Emitable, NlasIterator};
use netlink_packet_ethtool::{constants::*, StatsAttr};

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
    let dev = std::env::args().nth(1).unwrap();
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
    let attrs = StatsAttr::request(&dev, groups);
    let mut wire = vec![0u8; attrs.as_slice().buffer_len()];
    attrs.as_slice().emit(&mut wire);

    match handle.raw(ETHTOOL_MSG_STATS_GET, wire).await {
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
