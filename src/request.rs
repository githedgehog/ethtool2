// SPDX-License-Identifier: MIT

//! Building requests, apart from sending them, so the flags can be tested.
//!
//! Two layers of flags decide what comes back, and both have caught people
//! out:
//!
//! * the **netlink** header's flags: a `doit` request without `NLM_F_ACK`
//!   gets its reply but no terminating message, so a response stream waiting
//!   for the end never ends;
//! * the **ethtool** header's [`RequestFlags`]: without
//!   [`RequestFlags::stats`] the kernel omits every optional statistics nest,
//!   silently.

use netlink_packet_core::{NetlinkHeader, NetlinkMessage, NLM_F_ACK, NLM_F_DUMP, NLM_F_REQUEST};
use netlink_packet_ethtool::{
    Bitset, BitsetBit, EepromWindow, EthtoolCmd, EthtoolMessage, FeaturesAttr, FecAttr,
    LinkInfoAttr, LinkModesAttr, LinkStateAttr, ModuleEepromAttr, PauseAttr, PrivFlagsAttr,
    RequestFlags, StatsAttr, StatsGroupId, StringSet, StrsetAttr,
};
use netlink_packet_generic::GenlMessage;

use crate::Device;

/// The netlink header flags for a request.
///
/// A dump ends with `NLMSG_DONE`. A `doit` ends with nothing at all unless
/// it asks for an acknowledgement, so `NLM_F_ACK` is not optional there.
pub(crate) fn netlink_flags(dump: bool) -> u16 {
    if dump {
        NLM_F_REQUEST | NLM_F_DUMP
    } else {
        NLM_F_REQUEST | NLM_F_ACK
    }
}

/// Wrap a generic netlink payload in a netlink message with the right flags.
pub(crate) fn netlink_message<F>(payload: F, dump: bool) -> NetlinkMessage<GenlMessage<F>>
where
    F: netlink_packet_generic::GenlFamily + netlink_packet_core::Emitable + std::fmt::Debug,
{
    let mut header = NetlinkHeader::default();
    header.flags = netlink_flags(dump);
    NetlinkMessage::new(header, GenlMessage::from_payload(payload).into())
}

/// The ethtool flags for a `GET` that may carry an optional statistics nest.
fn stats_flags(stats: bool) -> RequestFlags {
    RequestFlags {
        stats,
        ..Default::default()
    }
}

pub(crate) fn link_info(device: &Device) -> EthtoolMessage {
    EthtoolMessage::LinkInfo(
        EthtoolCmd::LinkinfoGet,
        vec![LinkInfoAttr::Header(device.header(RequestFlags::default()))],
    )
}

pub(crate) fn link_modes(device: &Device) -> EthtoolMessage {
    EthtoolMessage::LinkModes(
        EthtoolCmd::LinkmodesGet,
        vec![LinkModesAttr::Header(
            device.header(RequestFlags::default()),
        )],
    )
}

pub(crate) fn link_state(device: &Device) -> EthtoolMessage {
    EthtoolMessage::LinkState(
        EthtoolCmd::LinkstateGet,
        vec![LinkStateAttr::Header(
            device.header(RequestFlags::default()),
        )],
    )
}

pub(crate) fn pause(device: &Device, stats: bool) -> EthtoolMessage {
    EthtoolMessage::Pause(
        EthtoolCmd::PauseGet,
        vec![PauseAttr::Header(device.header(stats_flags(stats)))],
    )
}

pub(crate) fn features(device: &Device) -> EthtoolMessage {
    EthtoolMessage::Features(
        EthtoolCmd::FeaturesGet,
        vec![FeaturesAttr::Header(device.header(RequestFlags::default()))],
    )
}

pub(crate) fn private_flags(device: &Device) -> EthtoolMessage {
    EthtoolMessage::PrivFlags(
        EthtoolCmd::PrivflagsGet,
        vec![PrivFlagsAttr::Header(
            device.header(RequestFlags::default()),
        )],
    )
}

pub(crate) fn fec(device: &Device, stats: bool) -> EthtoolMessage {
    EthtoolMessage::Fec(
        EthtoolCmd::FecGet,
        vec![FecAttr::Header(device.header(stats_flags(stats)))],
    )
}

/// The name the kernel resolves a group by, against `ETH_SS_STATS_STD`.
pub(crate) fn stats_group_name(group: StatsGroupId) -> Option<&'static str> {
    match group {
        StatsGroupId::EthPhy => Some("eth-phy"),
        StatsGroupId::EthMac => Some("eth-mac"),
        StatsGroupId::EthCtrl => Some("eth-ctrl"),
        StatsGroupId::Rmon => Some("rmon"),
        _ => None,
    }
}

/// A `STATS_GET` for `groups`.
///
/// Known groups are selected **by name**, as `ethtool(8)` does, because the
/// group ids are in no uapi header. A group this crate cannot name is
/// selected by its id.
///
/// No [`RequestFlags::stats`]: `STATS_GET` is the statistics command and
/// does not need asking.
pub(crate) fn stats(device: &Device, groups: &[StatsGroupId]) -> EthtoolMessage {
    let bits = groups
        .iter()
        .map(|group| match stats_group_name(*group) {
            Some(name) => BitsetBit {
                index: None,
                name: Some(name.to_string()),
                value: true,
            },
            None => BitsetBit {
                index: Some(u32::from(*group)),
                name: None,
                value: true,
            },
        })
        .collect();
    EthtoolMessage::Stats(
        EthtoolCmd::StatsGet,
        vec![
            StatsAttr::Header(device.header(RequestFlags::default())),
            StatsAttr::Groups(Bitset {
                nomask: true,
                bits,
                ..Default::default()
            }),
        ],
    )
}

/// A `STRSET_GET` for the sets `ids`, of `device` or, with `None`, the
/// global ones.
pub(crate) fn string_sets(device: Option<&Device>, ids: &[u32]) -> EthtoolMessage {
    let mut attributes = Vec::new();
    if let Some(device) = device {
        attributes.push(StrsetAttr::Header(device.header(RequestFlags::default())));
    }
    attributes.push(StrsetAttr::StringSets(
        ids.iter()
            .map(|id| StringSet {
                id: Some(*id),
                count: None,
                strings: Vec::new(),
            })
            .collect(),
    ));
    EthtoolMessage::Strset(EthtoolCmd::StrsetGet, attributes)
}

pub(crate) fn module_eeprom(device: &Device, window: EepromWindow) -> EthtoolMessage {
    let attributes = match device {
        Device::Name(name) => ModuleEepromAttr::request(name, window),
        Device::Index(index) => ModuleEepromAttr::request_by_index(*index, window),
    };
    EthtoolMessage::ModuleEeprom(EthtoolCmd::ModuleEepromGet, attributes)
}

#[cfg(test)]
mod tests {
    use netlink_packet_core::NetlinkPayload;
    use netlink_packet_ethtool::HeaderAttr;

    use super::*;

    fn header_of(message: &EthtoolMessage) -> Vec<HeaderAttr> {
        match message {
            EthtoolMessage::LinkInfo(_, a) => first_header(a, |a| match a {
                LinkInfoAttr::Header(h) => Some(h),
                _ => None,
            }),
            EthtoolMessage::LinkModes(_, a) => first_header(a, |a| match a {
                LinkModesAttr::Header(h) => Some(h),
                _ => None,
            }),
            EthtoolMessage::LinkState(_, a) => first_header(a, |a| match a {
                LinkStateAttr::Header(h) => Some(h),
                _ => None,
            }),
            EthtoolMessage::Pause(_, a) => first_header(a, |a| match a {
                PauseAttr::Header(h) => Some(h),
                _ => None,
            }),
            EthtoolMessage::Features(_, a) => first_header(a, |a| match a {
                FeaturesAttr::Header(h) => Some(h),
                _ => None,
            }),
            EthtoolMessage::PrivFlags(_, a) => first_header(a, |a| match a {
                PrivFlagsAttr::Header(h) => Some(h),
                _ => None,
            }),
            EthtoolMessage::Fec(_, a) => first_header(a, |a| match a {
                FecAttr::Header(h) => Some(h),
                _ => None,
            }),
            EthtoolMessage::Stats(_, a) => first_header(a, |a| match a {
                StatsAttr::Header(h) => Some(h),
                _ => None,
            }),
            EthtoolMessage::Strset(_, a) => first_header(a, |a| match a {
                StrsetAttr::Header(h) => Some(h),
                _ => None,
            }),
            EthtoolMessage::ModuleEeprom(_, a) => first_header(a, |a| match a {
                ModuleEepromAttr::Header(h) => Some(h),
                _ => None,
            }),
            other => panic!("no header in {other:?}"),
        }
    }

    fn first_header<A>(
        attributes: &[A],
        header: impl Fn(&A) -> Option<&Vec<HeaderAttr>>,
    ) -> Vec<HeaderAttr> {
        attributes
            .iter()
            .find_map(header)
            .cloned()
            .expect("a header attribute")
    }

    fn flags_of(message: &EthtoolMessage) -> RequestFlags {
        header_of(message)
            .into_iter()
            .find_map(|a| match a {
                HeaderAttr::Flags(f) => Some(f),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Every `GET` this crate sends, for one device.
    fn every_get(device: &Device) -> Vec<EthtoolMessage> {
        let window = EepromWindow::upper(0x50, 0, 0).unwrap();
        vec![
            link_info(device),
            link_modes(device),
            link_state(device),
            pause(device, false),
            pause(device, true),
            features(device),
            private_flags(device),
            fec(device, false),
            fec(device, true),
            stats(device, &[StatsGroupId::EthMac]),
            string_sets(Some(device), &[1]),
            module_eeprom(device, window),
        ]
    }

    #[test]
    fn doit_asks_for_an_ack() {
        assert_eq!(netlink_flags(false), NLM_F_REQUEST | NLM_F_ACK);
        let message = netlink_message(link_state(&Device::from("eth0")), false);
        assert_eq!(message.header.flags, NLM_F_REQUEST | NLM_F_ACK);
        assert!(matches!(message.payload, NetlinkPayload::InnerMessage(_)));
    }

    #[test]
    fn dump_asks_for_a_dump_not_an_ack() {
        assert_eq!(netlink_flags(true), NLM_F_REQUEST | NLM_F_DUMP);
        assert_eq!(netlink_flags(true) & NLM_F_ACK, 0);
    }

    #[test]
    fn requests_name_the_device_they_were_given() {
        bolero::check!()
            .with_type::<(String, u32)>()
            .for_each(|(name, index)| {
                for device in [Device::Name(name.clone()), Device::Index(*index)] {
                    for message in every_get(&device) {
                        let identity: Vec<_> = header_of(&message)
                            .into_iter()
                            .filter(|a| !matches!(a, HeaderAttr::Flags(_)))
                            .collect();
                        assert_eq!(identity, device.header(RequestFlags::default()));
                    }
                }
            });
    }

    #[test]
    fn statistics_are_asked_for_exactly_when_wanted() {
        let device = Device::from("eth0");
        assert!(flags_of(&pause(&device, true)).stats);
        assert!(!flags_of(&pause(&device, false)).stats);
        assert!(flags_of(&fec(&device, true)).stats);
        assert!(!flags_of(&fec(&device, false)).stats);
        // STATS_GET is the statistics command; ethtool(8) sends no flags.
        assert_eq!(
            flags_of(&stats(&device, &crate::STANDARD_STATS_GROUPS)),
            RequestFlags::default()
        );
    }

    #[test]
    fn no_request_asks_for_compact_bitsets() {
        // Compact bitsets carry no names, and every view resolves by name.
        for message in every_get(&Device::from("eth0")) {
            assert!(!flags_of(&message).compact_bitsets, "{message:?}");
            assert!(!flags_of(&message).omit_reply, "{message:?}");
        }
    }

    #[test]
    fn stats_groups_are_selected_by_name_or_id() {
        let message = stats(
            &Device::from("eth0"),
            &[StatsGroupId::EthMac, StatsGroupId::Other(9)],
        );
        let EthtoolMessage::Stats(EthtoolCmd::StatsGet, attributes) = message else {
            panic!("not a stats request");
        };
        let groups = attributes
            .iter()
            .find_map(|a| match a {
                StatsAttr::Groups(b) => Some(b.clone()),
                _ => None,
            })
            .unwrap();
        assert!(groups.nomask);
        assert_eq!(
            groups.bits,
            vec![
                BitsetBit {
                    index: None,
                    name: Some("eth-mac".into()),
                    value: true
                },
                BitsetBit {
                    index: Some(9),
                    name: None,
                    value: true
                },
            ]
        );
    }

    #[test]
    fn global_string_sets_carry_no_header() {
        let EthtoolMessage::Strset(_, attributes) = string_sets(None, &[4, 9]) else {
            panic!("not a strset request");
        };
        assert!(!attributes
            .iter()
            .any(|a| matches!(a, StrsetAttr::Header(_))));
        let ids: Vec<_> = attributes
            .iter()
            .flat_map(|a| match a {
                StrsetAttr::StringSets(sets) => sets.iter().map(|s| s.id).collect(),
                _ => Vec::new(),
            })
            .collect();
        assert_eq!(ids, vec![Some(4), Some(9)]);
    }

    #[test]
    fn module_eeprom_sends_the_whole_window() {
        let window = EepromWindow::new(0x50, 0x11, 1, 130, 16).unwrap();
        let EthtoolMessage::ModuleEeprom(cmd, attributes) = module_eeprom(&Device::from(6), window)
        else {
            panic!("not a module eeprom request");
        };
        assert_eq!(cmd, EthtoolCmd::ModuleEepromGet);
        assert_eq!(
            attributes,
            vec![
                ModuleEepromAttr::Header(vec![HeaderAttr::DevIndex(6)]),
                ModuleEepromAttr::Offset(130),
                ModuleEepromAttr::Length(16),
                ModuleEepromAttr::Page(0x11),
                ModuleEepromAttr::Bank(1),
                ModuleEepromAttr::I2cAddress(0x50),
            ]
        );
    }
}
