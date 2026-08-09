// SPDX-License-Identifier: MIT
//! Minimal handle, enough to talk to the kernel while the packet crate is
//! being verified.
use futures::stream::StreamExt;
use genetlink::GenetlinkHandle;
use netlink_packet_core::{
    NetlinkHeader, NetlinkMessage, NetlinkPayload, NLM_F_ACK, NLM_F_DUMP, NLM_F_REQUEST,
};
use netlink_packet_ethtool::EthtoolMessage;
use netlink_packet_generic::GenlMessage;

#[derive(Clone)]
pub struct Handle(GenetlinkHandle);

pub fn new_connection() -> std::io::Result<(impl std::future::Future<Output = ()>, Handle)> {
    let (connection, handle, _) = genetlink::new_connection()?;
    Ok((connection, Handle(handle)))
}

impl Handle {
    pub async fn request(
        &self,
        message: EthtoolMessage,
        dump: bool,
    ) -> Result<Vec<EthtoolMessage>, String> {
        let mut header = NetlinkHeader::default();
        header.flags = if dump {
            NLM_F_REQUEST | NLM_F_DUMP
        } else {
            NLM_F_REQUEST | NLM_F_ACK
        };
        let message = NetlinkMessage::new(header, GenlMessage::from_payload(message).into());
        let mut handle = self.0.clone();
        let mut responses = handle.request(message).await.map_err(|e| e.to_string())?;
        let mut collected = Vec::new();
        while let Some(result) = responses.next().await {
            match result.map_err(|e| e.to_string())?.payload {
                NetlinkPayload::InnerMessage(inner) => collected.push(inner.payload),
                NetlinkPayload::Error(e) => {
                    if let Some(code) = e.code {
                        return Err(format!("kernel: {code}"));
                    }
                }
                _ => {}
            }
        }
        Ok(collected)
    }
}

/// An ethtool message that is never interpreted: raw attribute bytes in,
/// raw attribute bytes out.
///
/// Only for probing. When the typed decoder and the kernel disagree, this is
/// how to find out which one is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opaque {
    pub cmd: u8,
    pub payload: Vec<u8>,
}

impl netlink_packet_generic::GenlFamily for Opaque {
    fn family_name() -> &'static str {
        netlink_packet_ethtool::ETHTOOL_GENL_NAME
    }
    fn command(&self) -> u8 {
        self.cmd
    }
    fn version(&self) -> u8 {
        1
    }
}

impl netlink_packet_core::Emitable for Opaque {
    fn buffer_len(&self) -> usize {
        self.payload.len()
    }
    fn emit(&self, buffer: &mut [u8]) {
        buffer[..self.payload.len()].copy_from_slice(&self.payload)
    }
}

impl netlink_packet_core::ParseableParametrized<[u8], netlink_packet_generic::GenlHeader>
    for Opaque
{
    fn parse_with_param(
        buffer: &[u8],
        header: netlink_packet_generic::GenlHeader,
    ) -> Result<Self, netlink_packet_core::DecodeError> {
        Ok(Self {
            cmd: header.cmd,
            payload: buffer.to_vec(),
        })
    }
}

impl Handle {
    /// Send raw attribute bytes and return the raw replies.
    pub async fn raw(&self, cmd: u8, payload: Vec<u8>) -> Result<Vec<Opaque>, String> {
        let mut header = NetlinkHeader::default();
        header.flags = NLM_F_REQUEST | NLM_F_ACK;
        let message = NetlinkMessage::new(
            header,
            GenlMessage::from_payload(Opaque { cmd, payload }).into(),
        );
        let mut handle = self.0.clone();
        let mut responses = handle.request(message).await.map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        while let Some(result) = responses.next().await {
            match result.map_err(|e| e.to_string())?.payload {
                NetlinkPayload::InnerMessage(inner) => out.push(inner.payload),
                NetlinkPayload::Error(e) => {
                    if let Some(code) = e.code {
                        return Err(format!("kernel: {code}"));
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }
}
