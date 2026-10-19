// SPDX-License-Identifier: MIT

//! Query Linux network devices through the `ethtool` generic netlink family.
//!
//! ```no_run
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use ethtool2 as ethtool;
//!
//! let (connection, handle) = ethtool::new_connection()?;
//! tokio::spawn(connection);
//!
//! let modes = handle.link_modes("eth0").await?;
//! let state = handle.link_state("eth0").await?;
//! println!("{:?} Mb/s, link {:?}", modes.speed, state.link);
//!
//! // Counters only arrive when asked for; the `_with_stats` methods ask.
//! let pause = handle.pause_with_stats("eth0").await?;
//! println!("{:?}", pause.stats);
//! # Ok(())
//! # }
//! ```
//!
//! Devices are named by [`Device`]: a `&str` or `String` is a name, a `u32`
//! an interface index.
//!
//! Each method is one `ethtool(8)` query and returns a typed view: named
//! fields for what the crate understands, plus every attribute the kernel
//! sent, so nothing is lost to a field this crate does not have yet. For
//! anything without a method, [`Handle::request`] takes a message built with
//! the packet crate, re-exported as [`packet_ethtool`]; its module docs record
//! the wire-format traps, such as statistics arriving only when the request
//! sets [`RequestFlags::stats`]. [`Handle::raw`] goes one step further and
//! skips decoding altogether.
//!
//! Everything here is a `GET`. Only [`Handle::module_eeprom`] needs a
//! capability: `CAP_NET_ADMIN`.

pub use netlink_packet_ethtool as packet_ethtool;

mod errors;
mod handle;
mod raw;
mod request;
mod views;

pub use netlink_packet_ethtool::{
    Bitset, BitsetBit, Device, Duplex, EepromWindow, FecCounter, FecHistBin, FecStats, HistBucket,
    LinkExtState, PauseStats, Port, RequestFlags, StatsGroupId, StringEntry, StringSet,
    WindowError, STANDARD_STATS_GROUPS,
};

pub use self::{
    errors::Error,
    handle::Handle,
    raw::RawMessage,
    views::{
        Counter, Feature, Features, Fec, GroupStats, LinkInfo, LinkModes, LinkState, Pause,
        PrivateFlags, ReplyDevice, Stats, StringSets,
    },
};

/// Open a connection to the ethtool family.
///
/// Spawn the returned future, then send requests through the handle. The
/// family id is resolved on the first request.
#[cfg(feature = "tokio_socket")]
pub fn new_connection() -> std::io::Result<(impl std::future::Future<Output = ()>, Handle)> {
    let (connection, genl_handle, _messages) = genetlink::new_connection()?;
    Ok((connection, Handle::from_genetlink(genl_handle)))
}
