// SPDX-License-Identifier: MIT

use std::fmt::Debug;

use futures::stream::StreamExt;
use genetlink::GenetlinkHandle;
use netlink_packet_core::{Emitable, NetlinkPayload, ParseableParametrized};
use netlink_packet_ethtool::{
    EepromWindow, EthtoolCmd, EthtoolMessage, ModuleEepromAttr, StatsGroupId,
};
use netlink_packet_generic::{GenlFamily, GenlHeader};

use crate::{
    request, Device, Error, Features, Fec, LinkInfo, LinkModes, LinkState, Pause, PrivateFlags,
    RawMessage, Stats, StringSets,
};

/// A connection to the kernel's ethtool family.
///
/// Cheap to clone; every clone shares the one socket.
#[derive(Clone)]
pub struct Handle(GenetlinkHandle);

impl Handle {
    /// Wrap a generic netlink handle from a connection opened some other
    /// way, e.g. on a runtime other than tokio, with the `tokio_socket`
    /// feature off.
    pub fn from_genetlink(handle: GenetlinkHandle) -> Self {
        Self(handle)
    }

    /// Send one message and collect every message the kernel answers with.
    ///
    /// Build `message` with the packet crate, e.g.
    /// `EthtoolMessage::Pause(EthtoolCmd::PauseGet, PauseAttr::request("eth0"))`.
    /// Use this for anything the typed methods do not cover.
    ///
    /// `dump` asks for every device (send a message without a device in its
    /// header). Otherwise the request is a `doit`, and is sent with
    /// `NLM_F_ACK`: a `doit` without it is answered but never terminated, so
    /// collecting its replies would wait forever.
    pub async fn request(
        &self,
        message: EthtoolMessage,
        dump: bool,
    ) -> Result<Vec<EthtoolMessage>, Error> {
        self.exchange(message, dump).await
    }

    /// Send raw attribute bytes under command number `cmd`, and return the
    /// replies undecoded.
    ///
    /// The escape hatch for **probing** an encoding: nothing on the way in
    /// or out is interpreted, so a reply the typed decoder gets wrong can be
    /// read as the kernel sent it. See [`RawMessage::encode`] to start from a
    /// typed request.
    pub async fn raw(&self, message: RawMessage, dump: bool) -> Result<Vec<RawMessage>, Error> {
        self.exchange(message, dump).await
    }

    async fn exchange<F>(&self, message: F, dump: bool) -> Result<Vec<F>, Error>
    where
        F: GenlFamily + Emitable + ParseableParametrized<[u8], GenlHeader> + Debug,
    {
        let mut handle = self.0.clone();
        let mut responses = handle
            .request(request::netlink_message(message, dump))
            .await
            .map_err(Error::Request)?;

        let mut collected = Vec::new();
        while let Some(result) = responses.next().await {
            match result.map_err(Error::Decode)?.payload {
                NetlinkPayload::InnerMessage(inner) => collected.push(inner.payload),
                NetlinkPayload::Error(e) if e.code.is_some() => {
                    return Err(Error::from_kernel(e));
                }
                // The acknowledgement, or the end of a dump.
                _ => {}
            }
        }
        Ok(collected)
    }

    /// Send a `doit` about `device` and keep the replies' attributes, or
    /// fail if there were none.
    async fn get<A>(
        &self,
        device: &Device,
        message: EthtoolMessage,
        attributes: impl Fn(EthtoolMessage) -> Option<Vec<A>>,
    ) -> Result<Vec<A>, Error> {
        let reply = message.cmd();
        let mut collected = None::<Vec<A>>;
        for message in self
            .exchange(message, false)
            .await
            .map_err(|e| e.about(device))?
        {
            if let Some(more) = attributes(message) {
                collected.get_or_insert_with(Vec::new).extend(more);
            }
        }
        collected.ok_or(Error::NoReply(reply))
    }

    /// Connector, PHY address and transceiver, the `Port:`, `PHYAD:` and
    /// `Transceiver:` lines of `ethtool <dev>`.
    pub async fn link_info(&self, device: impl Into<Device>) -> Result<LinkInfo, Error> {
        let device = device.into();
        self.get(&device, request::link_info(&device), |m| match m {
            EthtoolMessage::LinkInfo(_, a) => Some(a),
            _ => None,
        })
        .await
        .map(LinkInfo::from)
    }

    /// Speed, duplex, lanes, autonegotiation and the supported, advertised
    /// and partner link modes, the top of `ethtool <dev>`.
    pub async fn link_modes(&self, device: impl Into<Device>) -> Result<LinkModes, Error> {
        let device = device.into();
        self.get(&device, request::link_modes(&device), |m| match m {
            EthtoolMessage::LinkModes(_, a) => Some(a),
            _ => None,
        })
        .await
        .map(LinkModes::from)
    }

    /// Whether the carrier is up and, if not, why: `ethtool <dev>`'s
    /// `Link detected:` line.
    pub async fn link_state(&self, device: impl Into<Device>) -> Result<LinkState, Error> {
        let device = device.into();
        self.get(&device, request::link_state(&device), |m| match m {
            EthtoolMessage::LinkState(_, a) => Some(a),
            _ => None,
        })
        .await
        .map(LinkState::from)
    }

    /// Pause frame settings, `ethtool -a <dev>`.
    ///
    /// No counters; see [`Handle::pause_with_stats`].
    pub async fn pause(&self, device: impl Into<Device>) -> Result<Pause, Error> {
        self.pause_inner(device.into(), false).await
    }

    /// Pause frame settings and counters, `ethtool -I -a <dev>`.
    ///
    /// Sets `ETHTOOL_FLAG_STATS`, without which the kernel leaves the
    /// counters out of the reply entirely. [`Pause::stats`] is still `None`
    /// if the driver keeps no pause counters.
    pub async fn pause_with_stats(&self, device: impl Into<Device>) -> Result<Pause, Error> {
        self.pause_inner(device.into(), true).await
    }

    async fn pause_inner(&self, device: Device, stats: bool) -> Result<Pause, Error> {
        self.get(&device, request::pause(&device, stats), |m| match m {
            EthtoolMessage::Pause(_, a) => Some(a),
            _ => None,
        })
        .await
        .map(Pause::from)
    }

    /// Offload features, `ethtool -k <dev>`.
    pub async fn features(&self, device: impl Into<Device>) -> Result<Features, Error> {
        let device = device.into();
        self.get(&device, request::features(&device), |m| match m {
            EthtoolMessage::Features(_, a) => Some(a),
            _ => None,
        })
        .await
        .map(Features::from)
    }

    /// Driver private flags, `ethtool --show-priv-flags <dev>`.
    pub async fn private_flags(&self, device: impl Into<Device>) -> Result<PrivateFlags, Error> {
        let device = device.into();
        self.get(&device, request::private_flags(&device), |m| match m {
            EthtoolMessage::PrivFlags(_, a) => Some(a),
            _ => None,
        })
        .await
        .map(PrivateFlags::from)
    }

    /// FEC settings, `ethtool --show-fec <dev>`.
    ///
    /// No counters; see [`Handle::fec_with_stats`].
    pub async fn fec(&self, device: impl Into<Device>) -> Result<Fec, Error> {
        self.fec_inner(device.into(), false).await
    }

    /// FEC settings and counters, `ethtool -I --show-fec <dev>`.
    ///
    /// Sets `ETHTOOL_FLAG_STATS`, as for pause.
    pub async fn fec_with_stats(&self, device: impl Into<Device>) -> Result<Fec, Error> {
        self.fec_inner(device.into(), true).await
    }

    async fn fec_inner(&self, device: Device, stats: bool) -> Result<Fec, Error> {
        self.get(&device, request::fec(&device, stats), |m| match m {
            EthtoolMessage::Fec(_, a) => Some(a),
            _ => None,
        })
        .await
        .map(Fec::from)
    }

    /// Standardised statistics with every counter named,
    /// `ethtool -S <dev> --groups <groups>`.
    ///
    /// Pass [`crate::STANDARD_STATS_GROUPS`] for all four. A group the driver
    /// does not implement is simply absent from the reply.
    ///
    /// Two requests: `STATS_GET` for the values, then one `STRSET_GET` for
    /// the names of every group that came back, since a statistics reply
    /// carries only each counter's index into its group's string set.
    pub async fn stats(
        &self,
        device: impl Into<Device>,
        groups: &[StatsGroupId],
    ) -> Result<Stats, Error> {
        let device = device.into();
        let mut stats = self
            .get(&device, request::stats(&device, groups), |m| match m {
                EthtoolMessage::Stats(_, a) => Some(a),
                _ => None,
            })
            .await
            .map(Stats::from)?;
        let ids = stats.string_set_ids();
        if !ids.is_empty() {
            let names = self.string_sets(&device, &ids).await?;
            stats.resolve_names(&names);
        }
        Ok(stats)
    }

    /// String sets of `device`, by id: the names behind everything ethtool
    /// reports by index. `ethtool --show-...` has no direct equivalent; it
    /// is what `ethtool -S` and friends use underneath.
    ///
    /// Set ids are `ETH_SS_*`, in
    /// [`netlink_packet_ethtool::kernel_only::stringset`]. Some sets are
    /// per-device (`ETH_SS_STATS`, `ETH_SS_PRIV_FLAGS`); for the global ones
    /// see [`Handle::global_string_sets`].
    pub async fn string_sets(
        &self,
        device: impl Into<Device>,
        ids: &[u32],
    ) -> Result<StringSets, Error> {
        let device = device.into();
        self.get(
            &device,
            request::string_sets(Some(&device), ids),
            |m| match m {
                EthtoolMessage::Strset(_, a) => Some(a),
                _ => None,
            },
        )
        .await
        .map(StringSets::from)
    }

    /// String sets that belong to no device, such as `ETH_SS_LINK_MODES` or
    /// `ETH_SS_FEATURES`.
    pub async fn global_string_sets(&self, ids: &[u32]) -> Result<StringSets, Error> {
        let mut collected = None::<Vec<_>>;
        for message in self
            .exchange(request::string_sets(None, ids), false)
            .await?
        {
            if let EthtoolMessage::Strset(_, attributes) = message {
                collected.get_or_insert_with(Vec::new).extend(attributes);
            }
        }
        collected
            .map(StringSets::from)
            .ok_or(Error::NoReply(EthtoolCmd::StrsetGet))
    }

    /// Read `window` of the memory of the module plugged into `device`, the
    /// bytes `ethtool -m <dev> page .. bank .. offset .. length .. raw on`
    /// prints.
    ///
    /// **Needs `CAP_NET_ADMIN`**; without it this fails with
    /// [`Error::PermissionDenied`]. Each read is a transaction on the
    /// module's two-wire bus. The bytes are not interpreted: which pages
    /// exist, and what they hold, is for the module's memory map (SFF-8472,
    /// SFF-8636, CMIS) to say.
    pub async fn module_eeprom(
        &self,
        device: impl Into<Device>,
        window: EepromWindow,
    ) -> Result<Vec<u8>, Error> {
        let device = device.into();
        let attributes = self
            .get(
                &device,
                request::module_eeprom(&device, window),
                |m| match m {
                    EthtoolMessage::ModuleEeprom(_, a) => Some(a),
                    _ => None,
                },
            )
            .await?;
        ModuleEepromAttr::data(&attributes)
            .map(<[u8]>::to_vec)
            .ok_or(Error::NoReply(EthtoolCmd::ModuleEepromGet))
    }
}
