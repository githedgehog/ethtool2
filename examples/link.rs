// SPDX-License-Identifier: MIT
//! Link modes and link state, the equivalent of the top of `ethtool <dev>`.
use ethtool2::Duplex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args().nth(1).ok_or("usage: link <dev>")?;
    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    println!("Settings for {dev}:");

    let modes = handle.link_modes(&dev).await?;
    // The link-mode space also holds Autoneg, connector, pause and FEC bits;
    // ethtool(8) prints those on lines of their own, these lists do not.
    println!("\tSupported link modes: {}", modes.supported().join(" "));
    let advertised = modes.advertised();
    println!("\tAdvertised link modes: {}", advertised.join(" "));
    if let Some(ours) = &modes.ours {
        println!(
            "\t  ({} modes, {} encoding)",
            advertised.len(),
            if ours.is_compact() {
                "compact"
            } else {
                "verbose"
            }
        );
    }
    if modes.peer.is_some() {
        println!(
            "\tLink partner advertised: {}",
            modes.partner_advertised().join(" ")
        );
    }
    match modes.speed {
        Some(mbps) => println!("\tSpeed: {mbps}Mb/s"),
        None => println!("\tSpeed: Unknown!"),
    }
    if let Some(lanes) = modes.lanes {
        println!("\tLanes: {lanes}");
    }
    if let Some(duplex) = modes.duplex {
        println!(
            "\tDuplex: {}",
            match duplex {
                Duplex::Full => "Full".to_string(),
                Duplex::Half => "Half".to_string(),
                other => format!("{other:?}"),
            }
        );
    }
    if let Some(on) = modes.autoneg {
        println!("\tAuto-negotiation: {}", if on { "on" } else { "off" });
    }

    let state = handle.link_state(&dev).await?;
    if let Some(up) = state.link {
        println!("\tLink detected: {}", if up { "yes" } else { "no" });
    }
    if let Some(why) = state.ext_state {
        println!("\t  (down because: {why:?})");
    }
    if let Some(n) = state.ext_down_count {
        println!("\tLink down events: {n}");
    }
    Ok(())
}
