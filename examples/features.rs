// SPDX-License-Identifier: MIT
//! Offload features, driver private flags and port info -- the equivalent of
//! `ethtool -k`, `--show-priv-flags`, and the physical part of `ethtool <dev>`.
use netlink_packet_ethtool::{
    EthtoolCmd, EthtoolMessage, FeaturesAttr, LinkInfoAttr, PrivFlagsAttr,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args().nth(1).unwrap();
    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    // --- features: four bitsets over one bit space ---
    let mut active: Vec<String> = Vec::new();
    let mut nochange: Vec<String> = Vec::new();
    let mut wanted: Vec<String> = Vec::new();
    // (name, hw-capable) for every named bit, set or not.
    let mut hw: Vec<(String, bool)> = Vec::new();
    for reply in handle
        .request(
            EthtoolMessage::Features(EthtoolCmd::FeaturesGet, FeaturesAttr::request(&dev)),
            false,
        )
        .await?
    {
        let EthtoolMessage::Features(_, attributes) = reply else {
            continue;
        };
        for attribute in &attributes {
            match attribute {
                FeaturesAttr::Active(b) => active = owned(b),
                FeaturesAttr::NoChange(b) => nochange = owned(b),
                FeaturesAttr::Wanted(b) => wanted = owned(b),
                FeaturesAttr::Hw(b) => {
                    hw = b
                        .entries()
                        .into_iter()
                        .map(|(n, v)| (n.to_string(), v))
                        .collect()
                }
                _ => {}
            }
        }
    }

    // The full space of named features comes from `hw`, whose bits are all
    // listed whether or not the device implements them. `active` and friends
    // are nomask lists carrying only what is set.
    println!("Features for {dev}:");
    for (name, hw_capable) in &hw {
        if name.is_empty() {
            continue; // an unnamed bit in the space
        }
        let on = active.contains(name);
        // ethtool's "[fixed]" is *not* the NOCHANGE bitset -- that is the
        // small constant set the kernel will never change for anyone
        // (NETIF_F_NEVER_CHANGE, one bit here). A feature is fixed for *this
        // device* when it is absent from `hw`, i.e. the driver does not offer
        // it as changeable.
        let fixed = !hw_capable || nochange.contains(name);
        let requested = wanted.contains(name);
        let mut note = String::new();
        if fixed {
            note.push_str(" [fixed]");
        } else if requested != on {
            note.push_str(if requested {
                " [requested on]"
            } else {
                " [requested off]"
            });
        }
        println!("{name}: {}{note}", if on { "on" } else { "off" });
    }

    // --- driver private flags ---
    println!("\nPrivate flags for {dev}:");
    for reply in handle
        .request(
            EthtoolMessage::PrivFlags(EthtoolCmd::PrivflagsGet, PrivFlagsAttr::request(&dev)),
            false,
        )
        .await?
    {
        let EthtoolMessage::PrivFlags(_, attributes) = reply else {
            continue;
        };
        for attribute in &attributes {
            if let PrivFlagsAttr::Flags(bitset) = attribute {
                for bit in &bitset.bits {
                    if let Some(name) = &bit.name {
                        println!("{name:<19}: {}", if bit.value { "on" } else { "off" });
                    }
                }
            }
        }
    }

    // --- port info ---
    println!("\nPort info for {dev}:");
    for reply in handle
        .request(
            EthtoolMessage::LinkInfo(EthtoolCmd::LinkinfoGet, LinkInfoAttr::request(&dev)),
            false,
        )
        .await?
    {
        let EthtoolMessage::LinkInfo(_, attributes) = reply else {
            continue;
        };
        for attribute in &attributes {
            match attribute {
                LinkInfoAttr::Port(p) => println!(
                    "\tPort: {}",
                    match p {
                        0 => "Twisted Pair".to_string(),
                        1 => "AUI".to_string(),
                        2 => "MII".to_string(),
                        3 => "FIBRE".to_string(),
                        4 => "BNC".to_string(),
                        5 => "Direct Attach Copper".to_string(),
                        other => format!("Other({other})"),
                    }
                ),
                LinkInfoAttr::PhyAddr(a) => println!("\tPHYAD: {a}"),
                LinkInfoAttr::Transceiver(t) => println!(
                    "\tTransceiver: {}",
                    if *t == 0 { "internal" } else { "external" }
                ),
                _ => {}
            }
        }
    }
    Ok(())
}

/// The set bits' names, owned.
fn owned(bitset: &netlink_packet_ethtool::Bitset) -> Vec<String> {
    bitset.names().into_iter().map(str::to_string).collect()
}
