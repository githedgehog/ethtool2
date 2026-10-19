// SPDX-License-Identifier: MIT

use netlink_packet_core::{DecodeError, Emitable, Nla, ParseableParametrized};
use netlink_packet_ethtool::{ETHTOOL_GENL_NAME, ETHTOOL_GENL_VERSION};
use netlink_packet_generic::{GenlFamily, GenlHeader};

/// An ethtool message that is never interpreted: a command number and raw
/// attribute bytes, in both directions.
///
/// This is for **probing**, through [`crate::Handle::raw`]. When the typed
/// decoder and the kernel disagree about an encoding, sending and receiving
/// bytes no decoder has touched is how to find out which one is wrong.
///
/// The command is a raw number in whichever numbering applies: a request
/// carries an `ETHTOOL_MSG_*_GET` number, and a reply comes back with the
/// separate `ETHTOOL_MSG_*_GET_REPLY` numbering (see
/// [`netlink_packet_ethtool::constants`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMessage {
    pub cmd: u8,
    /// The attributes, as they are on the wire, without the generic netlink
    /// header.
    pub payload: Vec<u8>,
}

impl RawMessage {
    pub fn new(cmd: u8, payload: Vec<u8>) -> Self {
        Self { cmd, payload }
    }

    /// Encode typed attributes, e.g. one of the packet crate's request
    /// builders, so the reply can be read raw.
    pub fn encode<T: Nla>(cmd: u8, attributes: &[T]) -> Self {
        let mut payload = vec![0; attributes.buffer_len()];
        attributes.emit(&mut payload);
        Self { cmd, payload }
    }
}

impl GenlFamily for RawMessage {
    fn family_name() -> &'static str {
        ETHTOOL_GENL_NAME
    }

    fn command(&self) -> u8 {
        self.cmd
    }

    fn version(&self) -> u8 {
        ETHTOOL_GENL_VERSION
    }
}

impl Emitable for RawMessage {
    fn buffer_len(&self) -> usize {
        self.payload.len()
    }

    fn emit(&self, buffer: &mut [u8]) {
        buffer[..self.payload.len()].copy_from_slice(&self.payload)
    }
}

impl ParseableParametrized<[u8], GenlHeader> for RawMessage {
    fn parse_with_param(buffer: &[u8], header: GenlHeader) -> Result<Self, DecodeError> {
        Ok(Self {
            cmd: header.cmd,
            payload: buffer.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use netlink_packet_ethtool::{constants::ETHTOOL_MSG_PAUSE_GET, EthtoolCmd, EthtoolMessage};

    use super::*;
    use crate::{request, Device};

    /// A raw message carrying a typed request's bytes is the same request on
    /// the wire.
    #[test]
    fn encodes_exactly_what_the_typed_message_would() {
        bolero::check!()
            .with_type::<(String, bool)>()
            .for_each(|(name, stats)| {
                let typed = request::pause(&Device::Name(name.clone()), *stats);
                let EthtoolMessage::Pause(EthtoolCmd::PauseGet, attributes) = &typed else {
                    unreachable!()
                };
                let raw = RawMessage::encode(ETHTOOL_MSG_PAUSE_GET, attributes);
                let mut typed_bytes = vec![0; typed.buffer_len()];
                typed.emit(&mut typed_bytes);
                assert_eq!(raw.payload, typed_bytes);
                assert_eq!(raw.command(), typed.command());
                assert_eq!(raw.version(), typed.version());
            });
    }

    #[test]
    fn parses_without_interpreting() {
        let bytes = [8, 0, 1, 0, 1, 2, 3, 4];
        let header = GenlHeader {
            cmd: 77,
            version: 1,
        };
        assert_eq!(
            RawMessage::parse_with_param(&bytes, header).unwrap(),
            RawMessage::new(77, bytes.to_vec())
        );
    }
}
