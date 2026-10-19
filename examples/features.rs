// SPDX-License-Identifier: MIT
//! Offload features, driver private flags and port info -- the equivalent of
//! `ethtool -k`, `--show-priv-flags`, and the physical part of `ethtool <dev>`.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = std::env::args().nth(1).ok_or("usage: features <dev>")?;
    let (connection, handle) = ethtool2::new_connection()?;
    tokio::spawn(connection);

    // --- features: four bitsets over one bit space ---
    println!("Features for {dev}:");
    for feature in handle.features(&dev).await?.features() {
        let note = if feature.fixed() {
            " [fixed]"
        } else if feature.wanted != feature.active {
            if feature.wanted {
                " [requested on]"
            } else {
                " [requested off]"
            }
        } else {
            ""
        };
        println!(
            "{}: {}{note}",
            feature.name,
            if feature.active { "on" } else { "off" }
        );
    }

    // --- driver private flags ---
    println!("\nPrivate flags for {dev}:");
    for (name, on) in handle.private_flags(&dev).await?.entries() {
        println!("{name:<19}: {}", if on { "on" } else { "off" });
    }

    // --- port info ---
    println!("\nPort info for {dev}:");
    let info = handle.link_info(&dev).await?;
    if let Some(port) = info.port {
        println!("\tPort: {port}");
    }
    if let Some(address) = info.phy_address {
        println!("\tPHYAD: {address}");
    }
    if let Some(transceiver) = info.transceiver {
        println!(
            "\tTransceiver: {}",
            if transceiver == 0 {
                "internal"
            } else {
                "external"
            }
        );
    }
    Ok(())
}
