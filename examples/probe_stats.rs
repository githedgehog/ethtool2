// SPDX-License-Identifier: MIT
//! Dump a raw STATS_GET reply so the `type-value` encoding can be read off
//! the wire rather than guessed at.
//!
//! Goes through [`ethtool2::Handle::request`] with a message built by the
//! packet crate, and prints the decoded groups without any view on top.
use ethtool2::packet_ethtool::{
    Bitset, Device, EthtoolCmd, EthtoolMessage, StatsAttr, STANDARD_STATS_GROUPS,
};
use netlink_packet_core::Emitable;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args()
        .nth(1)
        .ok_or("usage: probe_stats <dev> [group...]")?;
    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    // Names as typed, so a group the packet crate does not model can still
    // be asked for; the standard four by default.
    let groups: Vec<String> = std::env::args().skip(2).collect();
    let attrs = if groups.is_empty() {
        StatsAttr::request(&dev, STANDARD_STATS_GROUPS)
    } else {
        vec![
            StatsAttr::Header(Device::from(&dev).header(Default::default())),
            StatsAttr::Groups(Bitset::select_names(groups)),
        ]
    };
    // Show exactly what we send.
    let mut wire = vec![0u8; attrs.as_slice().buffer_len()];
    attrs.as_slice().emit(&mut wire);
    println!(
        "request {} bytes: {:02x?}",
        wire.len(),
        &wire[..wire.len().min(64)]
    );

    let replies = handle
        .request(EthtoolMessage::Stats(EthtoolCmd::StatsGet, attrs), false)
        .await?;
    println!("{} reply message(s)", replies.len());
    for reply in &replies {
        let EthtoolMessage::Stats(cmd, attributes) = reply else {
            println!("  (unmodelled: {:?})", reply.cmd());
            continue;
        };
        println!("cmd {cmd:?}, {} attribute(s)", attributes.len());
        for a in attributes {
            match a {
                StatsAttr::Group(g) => println!(
                    "  GRP id={:?} ss_id={:?} stats={} hist_rx={} hist_tx={}",
                    g.id,
                    g.string_set_id,
                    g.stats.len(),
                    g.hist_rx.len(),
                    g.hist_tx.len()
                ),
                other => println!("  {other:?}"),
            }
        }
    }
    Ok(())
}
