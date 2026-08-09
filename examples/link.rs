// SPDX-License-Identifier: MIT
//! Link modes and link state, the equivalent of the top of `ethtool <dev>`.
use netlink_packet_ethtool::{
    Duplex, EthtoolCmd, EthtoolMessage, LinkModesAttr, LinkStateAttr, SPEED_UNKNOWN,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args().nth(1).unwrap();
    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    println!("Settings for {dev}:");

    for reply in handle
        .request(
            EthtoolMessage::LinkModes(EthtoolCmd::LinkmodesGet, LinkModesAttr::request(&dev)),
            false,
        )
        .await?
    {
        let EthtoolMessage::LinkModes(_, attributes) = reply else {
            continue;
        };
        for attribute in &attributes {
            match attribute {
                LinkModesAttr::Ours(bitset) => {
                    let names = bitset.names();
                    println!("\tAdvertised link modes: {}", names.join(" "));
                    println!(
                        "\t  ({} modes, {} encoding)",
                        names.len(),
                        if bitset.is_compact() {
                            "compact"
                        } else {
                            "verbose"
                        }
                    );
                }
                LinkModesAttr::Peer(bitset) => {
                    println!("\tLink partner advertised: {}", bitset.names().join(" "))
                }
                LinkModesAttr::Speed(mbps) if *mbps == SPEED_UNKNOWN => {
                    println!("\tSpeed: Unknown!")
                }
                LinkModesAttr::Speed(mbps) => {
                    println!("\tSpeed: {mbps}Mb/s")
                }
                LinkModesAttr::Duplex(d) => println!(
                    "\tDuplex: {}",
                    match d {
                        Duplex::Full => "Full".to_string(),
                        Duplex::Half => "Half".to_string(),
                        other => format!("{other:?}"),
                    }
                ),
                LinkModesAttr::Autoneg(on) => {
                    println!("\tAuto-negotiation: {}", if *on { "on" } else { "off" })
                }
                LinkModesAttr::Lanes(n) => println!("\tLanes: {n}"),
                _ => {}
            }
        }
    }

    for reply in handle
        .request(
            EthtoolMessage::LinkState(EthtoolCmd::LinkstateGet, LinkStateAttr::request(&dev)),
            false,
        )
        .await?
    {
        let EthtoolMessage::LinkState(_, attributes) = reply else {
            continue;
        };
        for attribute in &attributes {
            match attribute {
                LinkStateAttr::Link(up) => {
                    println!("\tLink detected: {}", if *up { "yes" } else { "no" })
                }
                LinkStateAttr::ExtState(why) => {
                    println!("\t  (down because: {why:?})")
                }
                LinkStateAttr::ExtDownCnt(n) => {
                    println!("\tLink down events: {n}")
                }
                _ => {}
            }
        }
    }
    Ok(())
}
