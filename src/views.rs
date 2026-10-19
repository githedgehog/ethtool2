// SPDX-License-Identifier: MIT

//! Typed views over ethtool replies.
//!
//! These are lossless: every attribute the kernel sent is still in
//! `attributes`, in order, including ones no named field covers. Where an
//! attribute repeats, the named field holds the last one.
//!
//! Every view also says which device it describes, from the reply's header:
//! the kernel fills in both the name and the index whichever one the request
//! used, so a request by index learns the name and vice versa.

use std::fmt;

use netlink_packet_ethtool::{
    Bitset, Duplex, FeaturesAttr, FecAttr, FecStats, HeaderAttr, HistBucket, LinkExtState,
    LinkInfoAttr, LinkModesAttr, LinkStateAttr, PauseAttr, PauseStats, PrivFlagsAttr, StatsAttr,
    StatsGroup, StatsGroupId, StringSet, StrsetAttr, SPEED_UNKNOWN,
};

use crate::request::stats_group_name;

/// Which device a reply describes, from its header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct ReplyDevice {
    pub index: Option<u32>,
    pub name: Option<String>,
}

impl ReplyDevice {
    fn update(&mut self, header: &[HeaderAttr]) {
        for attribute in header {
            match attribute {
                HeaderAttr::DevIndex(index) => self.index = Some(*index),
                HeaderAttr::DevName(name) => self.name = Some(name.clone()),
                _ => {}
            }
        }
    }
}

/// The connector, `PORT_*` from `linux/ethtool.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Port {
    TwistedPair,
    Aui,
    Mii,
    Fibre,
    Bnc,
    DirectAttach,
    /// `PORT_NONE`: the port has no connector, e.g. a virtual device.
    None,
    /// `PORT_OTHER`.
    Other,
    /// A value this crate does not know.
    Unknown(u8),
}

impl From<u8> for Port {
    fn from(value: u8) -> Self {
        match value {
            0x00 => Self::TwistedPair,
            0x01 => Self::Aui,
            0x02 => Self::Mii,
            0x03 => Self::Fibre,
            0x04 => Self::Bnc,
            0x05 => Self::DirectAttach,
            0xef => Self::None,
            0xff => Self::Other,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for Port {
    /// The words `ethtool(8)` prints after `Port:`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TwistedPair => f.write_str("Twisted Pair"),
            Self::Aui => f.write_str("AUI"),
            Self::Mii => f.write_str("MII"),
            Self::Fibre => f.write_str("FIBRE"),
            Self::Bnc => f.write_str("BNC"),
            Self::DirectAttach => f.write_str("Direct Attach Copper"),
            Self::None => f.write_str("None"),
            Self::Other => f.write_str("Other"),
            Self::Unknown(value) => write!(f, "Unknown! ({value})"),
        }
    }
}

/// The physical side of a port, from `LINKINFO_GET`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct LinkInfo {
    pub device: ReplyDevice,
    pub port: Option<Port>,
    pub phy_address: Option<u8>,
    /// Measured MDI-X state, `ETH_TP_MDI_*`.
    pub tp_mdix: Option<u8>,
    /// Requested MDI-X state, `ETH_TP_MDI_*`.
    pub tp_mdix_ctrl: Option<u8>,
    /// `XCVR_*`: 0 internal, 1 external. Vestigial; modern drivers report
    /// internal whatever is plugged in.
    pub transceiver: Option<u8>,
    pub attributes: Vec<LinkInfoAttr>,
}

impl From<Vec<LinkInfoAttr>> for LinkInfo {
    fn from(attributes: Vec<LinkInfoAttr>) -> Self {
        let mut view = Self::default();
        for attribute in &attributes {
            match attribute {
                LinkInfoAttr::Header(h) => view.device.update(h),
                LinkInfoAttr::Port(v) => view.port = Some(Port::from(*v)),
                LinkInfoAttr::PhyAddr(v) => view.phy_address = Some(*v),
                LinkInfoAttr::TpMdix(v) => view.tp_mdix = Some(*v),
                LinkInfoAttr::TpMdixCtrl(v) => view.tp_mdix_ctrl = Some(*v),
                LinkInfoAttr::Transceiver(v) => view.transceiver = Some(*v),
                _ => {}
            }
        }
        view.attributes = attributes;
        view
    }
}

/// Speed, duplex, autonegotiation and link modes, from `LINKMODES_GET`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct LinkModes {
    pub device: ReplyDevice,
    /// Whether the link is autonegotiated.
    pub autoneg: Option<bool>,
    /// Megabits per second; `None` when absent or reported as unknown,
    /// which drivers do while the link is down.
    pub speed: Option<u32>,
    pub duplex: Option<Duplex>,
    pub lanes: Option<u32>,
    /// This end's modes: the mask is what it supports, the value what it
    /// advertises. See [`LinkModes::supported`] and
    /// [`LinkModes::advertised`].
    pub ours: Option<Bitset>,
    /// What the link partner advertises; absent with no partner or autoneg
    /// off.
    pub peer: Option<Bitset>,
    pub master_slave_cfg: Option<u8>,
    pub master_slave_state: Option<u8>,
    pub rate_matching: Option<u8>,
    pub attributes: Vec<LinkModesAttr>,
}

impl LinkModes {
    /// Every mode this end supports, by name.
    ///
    /// The link-mode bit space is not only speeds: it also holds `Autoneg`,
    /// connector types such as `FIBRE`, `Pause`/`Asym_Pause`, and the FEC
    /// encodings, which `ethtool(8)` prints on separate lines.
    pub fn supported(&self) -> Vec<&str> {
        self.ours
            .iter()
            .flat_map(|ours| ours.bits.iter().filter_map(|bit| bit.name.as_deref()))
            .collect()
    }

    /// Every mode this end advertises, by name. The same caveat as
    /// [`LinkModes::supported`] applies.
    pub fn advertised(&self) -> Vec<&str> {
        self.ours.as_ref().map(Bitset::names).unwrap_or_default()
    }

    /// Every mode the link partner advertises, by name.
    pub fn partner_advertised(&self) -> Vec<&str> {
        self.peer.as_ref().map(Bitset::names).unwrap_or_default()
    }
}

impl From<Vec<LinkModesAttr>> for LinkModes {
    fn from(attributes: Vec<LinkModesAttr>) -> Self {
        let mut view = Self::default();
        for attribute in &attributes {
            match attribute {
                LinkModesAttr::Header(h) => view.device.update(h),
                LinkModesAttr::Autoneg(v) => view.autoneg = Some(*v),
                LinkModesAttr::Speed(v) => view.speed = (*v != SPEED_UNKNOWN).then_some(*v),
                LinkModesAttr::Duplex(v) => view.duplex = Some(*v),
                LinkModesAttr::Lanes(v) => view.lanes = Some(*v),
                LinkModesAttr::Ours(v) => view.ours = Some(v.clone()),
                LinkModesAttr::Peer(v) => view.peer = Some(v.clone()),
                LinkModesAttr::MasterSlaveCfg(v) => view.master_slave_cfg = Some(*v),
                LinkModesAttr::MasterSlaveState(v) => view.master_slave_state = Some(*v),
                LinkModesAttr::RateMatching(v) => view.rate_matching = Some(*v),
                _ => {}
            }
        }
        view.attributes = attributes;
        view
    }
}

/// Whether the carrier is up, and why not, from `LINKSTATE_GET`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct LinkState {
    pub device: ReplyDevice,
    /// Whether the carrier is up; `None` when the driver cannot say.
    pub link: Option<bool>,
    pub sqi: Option<u32>,
    pub sqi_max: Option<u32>,
    /// Why the link is down, while it is.
    pub ext_state: Option<LinkExtState>,
    pub ext_substate: Option<u8>,
    /// How many times the link has gone down since the driver loaded.
    pub ext_down_count: Option<u32>,
    pub attributes: Vec<LinkStateAttr>,
}

impl From<Vec<LinkStateAttr>> for LinkState {
    fn from(attributes: Vec<LinkStateAttr>) -> Self {
        let mut view = Self::default();
        for attribute in &attributes {
            match attribute {
                LinkStateAttr::Header(h) => view.device.update(h),
                LinkStateAttr::Link(v) => view.link = Some(*v),
                LinkStateAttr::Sqi(v) => view.sqi = Some(*v),
                LinkStateAttr::SqiMax(v) => view.sqi_max = Some(*v),
                LinkStateAttr::ExtState(v) => view.ext_state = Some(*v),
                LinkStateAttr::ExtSubstate(v) => view.ext_substate = Some(*v),
                LinkStateAttr::ExtDownCnt(v) => view.ext_down_count = Some(*v),
                _ => {}
            }
        }
        view.attributes = attributes;
        view
    }
}

/// Pause frame (802.3x flow control) settings, from `PAUSE_GET`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Pause {
    pub device: ReplyDevice,
    /// Whether pause is negotiated with the partner. Not the link's
    /// autonegotiation.
    pub autoneg: Option<bool>,
    pub rx: Option<bool>,
    pub tx: Option<bool>,
    /// Present only when statistics were asked for, and then only the
    /// counters the driver keeps.
    pub stats: Option<PauseStats>,
    /// `enum ethtool_mac_stats_src`.
    pub stats_src: Option<u32>,
    pub attributes: Vec<PauseAttr>,
}

impl From<Vec<PauseAttr>> for Pause {
    fn from(attributes: Vec<PauseAttr>) -> Self {
        let mut view = Self::default();
        for attribute in &attributes {
            match attribute {
                PauseAttr::Header(h) => view.device.update(h),
                PauseAttr::Autoneg(v) => view.autoneg = Some(*v),
                PauseAttr::Rx(v) => view.rx = Some(*v),
                PauseAttr::Tx(v) => view.tx = Some(*v),
                PauseAttr::Stats(v) => view.stats = Some(*v),
                PauseAttr::StatsSrc(v) => view.stats_src = Some(*v),
                _ => {}
            }
        }
        view.attributes = attributes;
        view
    }
}

/// One offload feature, as `ethtool -k` prints it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Feature {
    pub name: String,
    /// On right now.
    pub active: bool,
    /// Asked for. Differs from `active` when the kernel could not comply:
    /// `ethtool -k`'s `[requested on]` / `[requested off]`.
    pub wanted: bool,
    /// The driver lets it be changed (it is in `hw`).
    pub changeable: bool,
    /// The kernel never lets it be changed (it is in `nochange`).
    pub never_changes: bool,
}

impl Feature {
    /// `ethtool -k`'s `[fixed]`.
    ///
    /// Mostly *not* the `nochange` bitset, which is the small constant set
    /// the kernel refuses for every device. A feature is fixed for a given
    /// device when its driver does not offer it as changeable.
    pub fn fixed(&self) -> bool {
        !self.changeable || self.never_changes
    }
}

/// Offload features, from `FEATURES_GET`: four bitsets over one space.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Features {
    pub device: ReplyDevice,
    /// What the driver lets be changed. Its mask lists every named feature,
    /// so this is also the feature space.
    pub hw: Option<Bitset>,
    pub wanted: Option<Bitset>,
    pub active: Option<Bitset>,
    pub nochange: Option<Bitset>,
    pub attributes: Vec<FeaturesAttr>,
}

impl Features {
    /// Every named feature and its state, in the kernel's order.
    pub fn features(&self) -> Vec<Feature> {
        let Some(hw) = &self.hw else {
            return Vec::new();
        };
        let has = |set: &Option<Bitset>, name: &str| {
            set.as_ref()
                .map(|set| set.contains_name(name))
                .unwrap_or(false)
        };
        hw.entries()
            .into_iter()
            .filter(|(name, _)| !name.is_empty())
            .map(|(name, changeable)| Feature {
                name: name.to_string(),
                active: has(&self.active, name),
                wanted: has(&self.wanted, name),
                changeable,
                never_changes: has(&self.nochange, name),
            })
            .collect()
    }

    /// One feature, by name.
    pub fn feature(&self, name: &str) -> Option<Feature> {
        self.features().into_iter().find(|f| f.name == name)
    }
}

impl From<Vec<FeaturesAttr>> for Features {
    fn from(attributes: Vec<FeaturesAttr>) -> Self {
        let mut view = Self::default();
        for attribute in &attributes {
            match attribute {
                FeaturesAttr::Header(h) => view.device.update(h),
                FeaturesAttr::Hw(v) => view.hw = Some(v.clone()),
                FeaturesAttr::Wanted(v) => view.wanted = Some(v.clone()),
                FeaturesAttr::Active(v) => view.active = Some(v.clone()),
                FeaturesAttr::NoChange(v) => view.nochange = Some(v.clone()),
                _ => {}
            }
        }
        view.attributes = attributes;
        view
    }
}

/// Driver private flags, from `PRIVFLAGS_GET`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct PrivateFlags {
    pub device: ReplyDevice,
    /// Every flag the driver has, named, with its state.
    pub flags: Option<Bitset>,
    pub attributes: Vec<PrivFlagsAttr>,
}

impl PrivateFlags {
    /// Every flag and whether it is on, in the driver's order.
    pub fn entries(&self) -> Vec<(&str, bool)> {
        self.flags.as_ref().map(Bitset::entries).unwrap_or_default()
    }

    /// Whether the named flag is on; `None` if the driver has no such flag.
    pub fn get(&self, name: &str) -> Option<bool> {
        self.entries()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, on)| on)
    }
}

impl From<Vec<PrivFlagsAttr>> for PrivateFlags {
    fn from(attributes: Vec<PrivFlagsAttr>) -> Self {
        let mut view = Self::default();
        for attribute in &attributes {
            match attribute {
                PrivFlagsAttr::Header(h) => view.device.update(h),
                PrivFlagsAttr::Flags(v) => view.flags = Some(v.clone()),
                _ => {}
            }
        }
        view.attributes = attributes;
        view
    }
}

/// The FEC encodings among the link-mode bits, with the kernel's names for
/// them (`enum ethtool_link_mode_bit_indices`, uapi).
const FEC_LINK_MODES: [(u32, &str); 4] = [(49, "None"), (50, "RS"), (51, "BASER"), (74, "LLRS")];

/// Forward error correction, from `FEC_GET`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Fec {
    pub device: ReplyDevice,
    /// The configured encodings, as link-mode bits.
    pub modes: Option<Bitset>,
    /// Whether the driver picks the encoding itself.
    pub auto: Option<bool>,
    /// The link-mode bit of the encoding in use; `None` when absent or zero,
    /// both meaning no FEC is active.
    pub active: Option<u32>,
    /// Present only when statistics were asked for.
    pub stats: Option<FecStats>,
    pub attributes: Vec<FecAttr>,
}

impl Fec {
    /// The configured encodings, by name. `ethtool(8)` adds `Auto` when
    /// [`Fec::auto`] is set and prints `None` for an empty list.
    pub fn configured(&self) -> Vec<&str> {
        self.modes.as_ref().map(Bitset::names).unwrap_or_default()
    }

    /// The name of the active encoding, e.g. `RS`.
    pub fn active_name(&self) -> Option<&str> {
        let active = self.active?;
        let from_modes = self.modes.iter().flat_map(|m| &m.bits).find_map(|bit| {
            (bit.index == Some(active))
                .then_some(bit.name.as_deref())
                .flatten()
        });
        from_modes.or_else(|| {
            FEC_LINK_MODES
                .iter()
                .find(|(bit, _)| *bit == active)
                .map(|(_, name)| *name)
        })
    }
}

impl From<Vec<FecAttr>> for Fec {
    fn from(attributes: Vec<FecAttr>) -> Self {
        let mut view = Self::default();
        for attribute in &attributes {
            match attribute {
                FecAttr::Header(h) => view.device.update(h),
                FecAttr::Modes(v) => view.modes = Some(v.clone()),
                FecAttr::Auto(v) => view.auto = Some(*v),
                FecAttr::Active(v) => view.active = (*v != 0).then_some(*v),
                FecAttr::Stats(v) => view.stats = Some(v.clone()),
                _ => {}
            }
        }
        view.attributes = attributes;
        view
    }
}

/// String sets, from `STRSET_GET`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct StringSets {
    pub device: ReplyDevice,
    pub sets: Vec<StringSet>,
    pub attributes: Vec<StrsetAttr>,
}

impl StringSets {
    /// The set with id `id`, if the reply carried it.
    pub fn get(&self, id: u32) -> Option<&StringSet> {
        self.sets.iter().find(|set| set.id == Some(id))
    }
}

impl From<Vec<StrsetAttr>> for StringSets {
    fn from(attributes: Vec<StrsetAttr>) -> Self {
        let mut view = Self::default();
        for attribute in &attributes {
            match attribute {
                StrsetAttr::Header(h) => view.device.update(h),
                StrsetAttr::StringSets(sets) => view.sets.extend(sets.iter().cloned()),
                _ => {}
            }
        }
        view.attributes = attributes;
        view
    }
}

/// One counter of a statistics group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Counter {
    /// The counter's index within its group's string set.
    pub index: u16,
    /// Its name, e.g. `FramesTransmittedOK`, once resolved.
    pub name: Option<String>,
    pub value: u64,
}

/// One standardised statistics group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct GroupStats {
    pub group: Option<StatsGroupId>,
    /// The string set naming this group's counters.
    pub string_set_id: Option<u32>,
    /// The counters the driver reported; indices are sparse.
    pub counters: Vec<Counter>,
    /// Received frame-size histogram (RMON only).
    pub hist_rx: Vec<HistBucket>,
    /// Transmitted frame-size histogram (RMON only).
    pub hist_tx: Vec<HistBucket>,
}

impl GroupStats {
    /// The group's name as `ethtool -S --groups` takes it, e.g. `eth-mac`.
    pub fn label(&self) -> Option<&'static str> {
        self.group.and_then(stats_group_name)
    }

    /// A counter's value, by name. Needs names resolved.
    pub fn get(&self, name: &str) -> Option<u64> {
        self.counters
            .iter()
            .find(|c| c.name.as_deref() == Some(name))
            .map(|c| c.value)
    }

    fn resolve(&mut self, sets: &StringSets) {
        let Some(set) = self.string_set_id.and_then(|id| sets.get(id)) else {
            return;
        };
        for counter in &mut self.counters {
            counter.name = set.name(u32::from(counter.index)).map(str::to_string);
        }
    }
}

impl From<&StatsGroup> for GroupStats {
    fn from(group: &StatsGroup) -> Self {
        Self {
            group: group.group(),
            string_set_id: group.string_set_id,
            counters: group
                .stats
                .iter()
                .map(|stat| Counter {
                    index: stat.index,
                    name: None,
                    value: stat.value,
                })
                .collect(),
            hist_rx: group.hist_rx.clone(),
            hist_tx: group.hist_tx.clone(),
        }
    }
}

/// Standardised statistics, from `STATS_GET`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Stats {
    pub device: ReplyDevice,
    pub groups: Vec<GroupStats>,
    /// `enum ethtool_mac_stats_src`.
    pub src: Option<u32>,
    pub attributes: Vec<StatsAttr>,
}

impl Stats {
    /// The ids of the string sets naming these groups' counters, once each.
    pub fn string_set_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = Vec::new();
        for id in self.groups.iter().filter_map(|g| g.string_set_id) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }

    /// Name every counter from `sets`, which should hold
    /// [`Stats::string_set_ids`]. Counters whose set or index is missing
    /// stay unnamed.
    pub fn resolve_names(&mut self, sets: &StringSets) {
        for group in &mut self.groups {
            group.resolve(sets);
        }
    }

    /// One group, if the reply carried it.
    pub fn group(&self, group: StatsGroupId) -> Option<&GroupStats> {
        self.groups.iter().find(|g| g.group == Some(group))
    }
}

impl From<Vec<StatsAttr>> for Stats {
    fn from(attributes: Vec<StatsAttr>) -> Self {
        let mut view = Self::default();
        for attribute in &attributes {
            match attribute {
                StatsAttr::Header(h) => view.device.update(h),
                StatsAttr::Group(group) => view.groups.push(GroupStats::from(group)),
                StatsAttr::Src(v) => view.src = Some(*v),
                _ => {}
            }
        }
        view.attributes = attributes;
        view
    }
}

#[cfg(test)]
mod tests {
    use netlink_packet_ethtool::{BitsetBit, FecCounter, Stat, StringEntry};

    use super::*;

    fn bit(index: u32, name: &str, value: bool) -> BitsetBit {
        BitsetBit {
            index: Some(index),
            name: Some(name.to_string()),
            value,
        }
    }

    /// A reply bitset with a mask: every listed bit is in the space, and the
    /// value flag says whether it is set.
    fn masked(bits: Vec<BitsetBit>) -> Bitset {
        Bitset {
            nomask: false,
            bits,
            ..Default::default()
        }
    }

    /// A reply bitset without a mask: the listed bits are the set ones.
    fn listed(bits: Vec<BitsetBit>) -> Bitset {
        Bitset {
            nomask: true,
            bits,
            ..Default::default()
        }
    }

    fn header(name: &str, index: u32) -> Vec<HeaderAttr> {
        vec![
            HeaderAttr::DevIndex(index),
            HeaderAttr::DevName(name.to_string()),
        ]
    }

    #[test]
    fn link_info() {
        let attributes = vec![
            LinkInfoAttr::Header(header("eth0", 5)),
            LinkInfoAttr::Port(3),
            LinkInfoAttr::PhyAddr(0),
            LinkInfoAttr::TpMdix(0),
            LinkInfoAttr::TpMdixCtrl(0),
            LinkInfoAttr::Transceiver(0),
        ];
        let view = LinkInfo::from(attributes.clone());
        assert_eq!(view.device.name.as_deref(), Some("eth0"));
        assert_eq!(view.device.index, Some(5));
        assert_eq!(view.port, Some(Port::Fibre));
        assert_eq!(view.port.unwrap().to_string(), "FIBRE");
        assert_eq!(view.phy_address, Some(0));
        assert_eq!(view.transceiver, Some(0));
        assert_eq!(view.attributes, attributes);
    }

    #[test]
    fn port_names() {
        assert_eq!(Port::from(5).to_string(), "Direct Attach Copper");
        assert_eq!(Port::from(0xef), Port::None);
        assert_eq!(Port::from(0xff), Port::Other);
        assert_eq!(Port::from(0x42), Port::Unknown(0x42));
    }

    #[test]
    fn link_modes() {
        let ours = masked(vec![
            bit(0, "10baseT/Half", false),
            bit(1, "10baseT/Full", true),
            bit(13, "FIBRE", true),
        ]);
        let peer = listed(vec![bit(1, "10baseT/Full", false)]);
        let attributes = vec![
            LinkModesAttr::Header(header("eth0", 5)),
            LinkModesAttr::Autoneg(true),
            LinkModesAttr::Ours(ours.clone()),
            LinkModesAttr::Peer(peer.clone()),
            LinkModesAttr::Speed(400_000),
            LinkModesAttr::Duplex(Duplex::Full),
            LinkModesAttr::Lanes(4),
        ];
        let view = LinkModes::from(attributes.clone());
        assert_eq!(view.autoneg, Some(true));
        assert_eq!(view.speed, Some(400_000));
        assert_eq!(view.duplex, Some(Duplex::Full));
        assert_eq!(view.lanes, Some(4));
        assert_eq!(
            view.supported(),
            vec!["10baseT/Half", "10baseT/Full", "FIBRE"]
        );
        assert_eq!(view.advertised(), vec!["10baseT/Full", "FIBRE"]);
        // A nomask bitset lists only set bits; the value flag is unused.
        assert_eq!(view.partner_advertised(), vec!["10baseT/Full"]);
        assert_eq!(view.ours, Some(ours));
        assert_eq!(view.attributes, attributes);
    }

    #[test]
    fn unknown_speed_is_none() {
        let view = LinkModes::from(vec![LinkModesAttr::Speed(SPEED_UNKNOWN)]);
        assert_eq!(view.speed, None);
        assert_eq!(view.attributes, vec![LinkModesAttr::Speed(SPEED_UNKNOWN)]);
    }

    #[test]
    fn link_state() {
        let attributes = vec![
            LinkStateAttr::Header(header("eth0", 5)),
            LinkStateAttr::Link(false),
            LinkStateAttr::ExtState(LinkExtState::NoCable),
            LinkStateAttr::ExtSubstate(0),
            LinkStateAttr::ExtDownCnt(3),
        ];
        let view = LinkState::from(attributes.clone());
        assert_eq!(view.link, Some(false));
        assert_eq!(view.ext_state, Some(LinkExtState::NoCable));
        assert_eq!(view.ext_substate, Some(0));
        assert_eq!(view.ext_down_count, Some(3));
        assert_eq!(view.sqi, None);
        assert_eq!(view.attributes, attributes);
    }

    #[test]
    fn pause() {
        let stats = PauseStats {
            tx_frames: Some(7),
            rx_frames: Some(0),
            tx_pause_storm_events: None,
        };
        let attributes = vec![
            PauseAttr::Header(header("eth0", 5)),
            PauseAttr::Autoneg(false),
            PauseAttr::Rx(true),
            PauseAttr::Tx(true),
            PauseAttr::Stats(stats),
        ];
        let view = Pause::from(attributes.clone());
        assert_eq!(view.autoneg, Some(false));
        assert_eq!(view.rx, Some(true));
        assert_eq!(view.tx, Some(true));
        assert_eq!(view.stats, Some(stats));
        assert_eq!(view.attributes, attributes);
    }

    #[test]
    fn pause_without_stats() {
        let view = Pause::from(vec![PauseAttr::Rx(true)]);
        assert_eq!(view.stats, None);
    }

    #[test]
    fn features() {
        // `hw` carries the whole space as its mask; the others list only
        // their set bits.
        let hw = masked(vec![
            bit(0, "rx-checksum", true),
            bit(1, "highdma", false),
            bit(2, "rx-gro-hw", true),
            bit(3, "tx-lockless", true),
            bit(4, "", false),
        ]);
        let attributes = vec![
            FeaturesAttr::Header(header("eth0", 5)),
            FeaturesAttr::Hw(hw),
            FeaturesAttr::Wanted(listed(vec![
                bit(0, "rx-checksum", true),
                bit(1, "highdma", true),
                bit(2, "rx-gro-hw", true),
            ])),
            FeaturesAttr::Active(listed(vec![
                bit(0, "rx-checksum", true),
                bit(1, "highdma", true),
            ])),
            FeaturesAttr::NoChange(listed(vec![bit(3, "tx-lockless", true)])),
        ];
        let view = Features::from(attributes.clone());
        let features = view.features();
        let names: Vec<_> = features.iter().map(|f| f.name.as_str()).collect();
        // The unnamed bit is skipped.
        assert_eq!(
            names,
            vec!["rx-checksum", "highdma", "rx-gro-hw", "tx-lockless"]
        );

        let rx_checksum = view.feature("rx-checksum").unwrap();
        assert!(rx_checksum.active && !rx_checksum.fixed());

        // On, but not changeable: `on [fixed]`.
        let highdma = view.feature("highdma").unwrap();
        assert!(highdma.active && highdma.fixed());

        // Asked for, not on: `off [requested on]`.
        let gro = view.feature("rx-gro-hw").unwrap();
        assert!(gro.wanted && !gro.active && !gro.fixed());

        // Changeable by the driver, refused by the kernel.
        let lockless = view.feature("tx-lockless").unwrap();
        assert!(lockless.changeable && lockless.never_changes && lockless.fixed());

        assert_eq!(view.attributes, attributes);
    }

    #[test]
    fn features_without_hw_are_empty() {
        let view = Features::from(vec![FeaturesAttr::Active(listed(vec![bit(0, "x", true)]))]);
        assert!(view.features().is_empty());
    }

    #[test]
    fn private_flags() {
        let attributes = vec![
            PrivFlagsAttr::Header(header("eth0", 5)),
            PrivFlagsAttr::Flags(masked(vec![
                bit(0, "rx_cqe_moder", false),
                bit(1, "rx_striding_rq", true),
            ])),
        ];
        let view = PrivateFlags::from(attributes.clone());
        assert_eq!(
            view.entries(),
            vec![("rx_cqe_moder", false), ("rx_striding_rq", true)]
        );
        assert_eq!(view.get("rx_striding_rq"), Some(true));
        assert_eq!(view.get("rx_cqe_moder"), Some(false));
        assert_eq!(view.get("nope"), None);
        assert_eq!(view.attributes, attributes);
    }

    #[test]
    fn fec() {
        let stats = FecStats {
            corrected: FecCounter {
                total: Some(10),
                lanes: vec![1, 2, 3, 4],
            },
            ..Default::default()
        };
        let attributes = vec![
            FecAttr::Header(header("eth0", 5)),
            FecAttr::Modes(listed(Vec::new())),
            FecAttr::Auto(true),
            FecAttr::Active(50),
            FecAttr::Stats(stats.clone()),
        ];
        let view = Fec::from(attributes.clone());
        assert_eq!(view.auto, Some(true));
        assert_eq!(view.active, Some(50));
        // Not among the configured modes, so named from the uapi bit.
        assert_eq!(view.active_name(), Some("RS"));
        assert!(view.configured().is_empty());
        assert_eq!(view.stats, Some(stats));
        assert_eq!(view.attributes, attributes);
    }

    #[test]
    fn fec_names_active_from_the_reply_first() {
        let view = Fec::from(vec![
            FecAttr::Modes(listed(vec![bit(51, "BaseR-as-sent", true)])),
            FecAttr::Active(51),
        ]);
        assert_eq!(view.configured(), vec!["BaseR-as-sent"]);
        assert_eq!(view.active_name(), Some("BaseR-as-sent"));
    }

    #[test]
    fn fec_active_zero_is_none() {
        let view = Fec::from(vec![FecAttr::Active(0)]);
        assert_eq!(view.active, None);
        assert_eq!(view.active_name(), None);
    }

    fn string_set(id: u32, names: &[(u32, &str)]) -> StringSet {
        StringSet {
            id: Some(id),
            count: Some(names.len() as u32),
            strings: names
                .iter()
                .map(|(index, name)| StringEntry {
                    index: Some(*index),
                    value: Some(name.to_string()),
                })
                .collect(),
        }
    }

    #[test]
    fn string_sets() {
        let attributes = vec![
            StrsetAttr::Header(header("eth0", 5)),
            StrsetAttr::StringSets(vec![
                string_set(18, &[(0, "FramesTransmittedOK")]),
                string_set(19, &[(0, "MACControlFramesTransmitted")]),
            ]),
        ];
        let view = StringSets::from(attributes.clone());
        assert_eq!(view.sets.len(), 2);
        assert_eq!(
            view.get(19).unwrap().name(0),
            Some("MACControlFramesTransmitted")
        );
        assert!(view.get(20).is_none());
        assert_eq!(view.attributes, attributes);
    }

    #[test]
    fn stats_with_names_resolved() {
        let mac = StatsGroup {
            id: Some(1),
            string_set_id: Some(18),
            stats: vec![
                Stat {
                    index: 0,
                    value: 100,
                },
                Stat { index: 3, value: 7 },
            ],
            ..Default::default()
        };
        let rmon = StatsGroup {
            id: Some(3),
            string_set_id: Some(20),
            stats: vec![Stat { index: 0, value: 1 }],
            hist_rx: vec![HistBucket {
                low: Some(0),
                high: Some(64),
                value: Some(5),
            }],
            ..Default::default()
        };
        let attributes = vec![
            StatsAttr::Header(header("eth0", 5)),
            StatsAttr::Group(mac),
            StatsAttr::Group(rmon),
        ];
        let mut view = Stats::from(attributes.clone());
        assert_eq!(view.string_set_ids(), vec![18, 20]);
        assert!(view
            .groups
            .iter()
            .all(|g| g.counters.iter().all(|c| c.name.is_none())));

        let names = StringSets::from(vec![StrsetAttr::StringSets(vec![
            string_set(
                18,
                &[(0, "FramesTransmittedOK"), (3, "OctetsTransmittedOK")],
            ),
            // A set lacking index 0: the counter stays unnamed.
            string_set(20, &[(1, "etherStatsFragments")]),
        ])]);
        view.resolve_names(&names);

        let mac = view.group(StatsGroupId::EthMac).unwrap();
        assert_eq!(mac.label(), Some("eth-mac"));
        assert_eq!(mac.get("FramesTransmittedOK"), Some(100));
        assert_eq!(mac.get("OctetsTransmittedOK"), Some(7));

        let rmon = view.group(StatsGroupId::Rmon).unwrap();
        assert_eq!(rmon.counters[0].name, None);
        assert_eq!(rmon.hist_rx.len(), 1);
        assert_eq!(view.attributes, attributes);
    }

    /// Generate attribute lists and check every view keeps them whole, and
    /// that a named field holds the last attribute of its kind.
    mod properties {
        use super::*;

        #[test]
        fn link_info_is_lossless() {
            bolero::check!()
                .with_type::<Vec<LinkInfoAttr>>()
                .for_each(|attributes| {
                    let view = LinkInfo::from(attributes.clone());
                    assert_eq!(&view.attributes, attributes);
                    let last = attributes.iter().rev().find_map(|a| match a {
                        LinkInfoAttr::Port(v) => Some(Port::from(*v)),
                        _ => None,
                    });
                    assert_eq!(view.port, last);
                });
        }

        #[test]
        fn link_modes_is_lossless() {
            bolero::check!()
                .with_type::<Vec<LinkModesAttr>>()
                .for_each(|attributes| {
                    let view = LinkModes::from(attributes.clone());
                    assert_eq!(&view.attributes, attributes);
                    let last = attributes.iter().rev().find_map(|a| match a {
                        LinkModesAttr::Speed(v) => Some(*v),
                        _ => None,
                    });
                    assert_eq!(view.speed, last.filter(|v| *v != SPEED_UNKNOWN));
                });
        }

        #[test]
        fn link_state_is_lossless() {
            bolero::check!()
                .with_type::<Vec<LinkStateAttr>>()
                .for_each(|attributes| {
                    let view = LinkState::from(attributes.clone());
                    assert_eq!(&view.attributes, attributes);
                    let last = attributes.iter().rev().find_map(|a| match a {
                        LinkStateAttr::Link(v) => Some(*v),
                        _ => None,
                    });
                    assert_eq!(view.link, last);
                });
        }

        #[test]
        fn pause_is_lossless() {
            bolero::check!()
                .with_type::<Vec<PauseAttr>>()
                .for_each(|attributes| {
                    let view = Pause::from(attributes.clone());
                    assert_eq!(&view.attributes, attributes);
                    let last = attributes.iter().rev().find_map(|a| match a {
                        PauseAttr::Stats(v) => Some(*v),
                        _ => None,
                    });
                    assert_eq!(view.stats, last);
                });
        }

        #[test]
        fn features_is_lossless() {
            bolero::check!()
                .with_type::<Vec<FeaturesAttr>>()
                .for_each(|attributes| {
                    let view = Features::from(attributes.clone());
                    assert_eq!(&view.attributes, attributes);
                    // Never more features than named bits in `hw`.
                    let named = view
                        .hw
                        .as_ref()
                        .map(|hw| hw.entries().iter().filter(|(n, _)| !n.is_empty()).count())
                        .unwrap_or(0);
                    assert_eq!(view.features().len(), named);
                });
        }

        #[test]
        fn private_flags_is_lossless() {
            bolero::check!()
                .with_type::<Vec<PrivFlagsAttr>>()
                .for_each(|attributes| {
                    let view = PrivateFlags::from(attributes.clone());
                    assert_eq!(&view.attributes, attributes);
                });
        }

        #[test]
        fn fec_is_lossless() {
            bolero::check!()
                .with_type::<Vec<FecAttr>>()
                .for_each(|attributes| {
                    let view = Fec::from(attributes.clone());
                    assert_eq!(&view.attributes, attributes);
                    let last = attributes.iter().rev().find_map(|a| match a {
                        FecAttr::Active(v) => Some(*v),
                        _ => None,
                    });
                    assert_eq!(view.active, last.filter(|v| *v != 0));
                });
        }

        #[test]
        fn string_sets_is_lossless() {
            bolero::check!()
                .with_type::<Vec<StrsetAttr>>()
                .for_each(|attributes| {
                    let view = StringSets::from(attributes.clone());
                    assert_eq!(&view.attributes, attributes);
                    let sets: usize = attributes
                        .iter()
                        .map(|a| match a {
                            StrsetAttr::StringSets(s) => s.len(),
                            _ => 0,
                        })
                        .sum();
                    assert_eq!(view.sets.len(), sets);
                });
        }

        #[test]
        fn stats_is_lossless() {
            bolero::check!()
                .with_type::<Vec<StatsAttr>>()
                .for_each(|attributes| {
                    let view = Stats::from(attributes.clone());
                    assert_eq!(&view.attributes, attributes);
                    let groups: Vec<&StatsGroup> = attributes
                        .iter()
                        .filter_map(|a| match a {
                            StatsAttr::Group(g) => Some(g),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(view.groups.len(), groups.len());
                    for (view, group) in view.groups.iter().zip(groups) {
                        let counters: Vec<_> =
                            view.counters.iter().map(|c| (c.index, c.value)).collect();
                        let stats: Vec<_> =
                            group.stats.iter().map(|s| (s.index, s.value)).collect();
                        assert_eq!(counters, stats);
                    }
                });
        }
    }
}
