// SPDX-License-Identifier: MIT

use std::fmt;

use netlink_packet_ethtool::{HeaderAttr, RequestFlags};

/// The network device a request is about, by name or by index.
///
/// Every ethtool request names its device in the common header, and the
/// kernel accepts either form:
///
/// * a **name** is what `ethtool(8)` takes and what people type;
/// * an **index** stays valid across a rename, and is the only stable handle
///   for a caller that learned about the device from rtnetlink, or that
///   knows it by a different name than the socket's namespace does.
///
/// Either way the lookup happens in the network namespace of the netlink
/// socket, not the caller's.
///
/// ```
/// use ethtool2::Device;
///
/// assert_eq!(Device::from("eth0"), Device::Name("eth0".into()));
/// assert_eq!(Device::from(5), Device::Index(5));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Device {
    /// `ETHTOOL_A_HEADER_DEV_NAME`.
    Name(String),
    /// `ETHTOOL_A_HEADER_DEV_INDEX`, the interface index.
    Index(u32),
}

impl Device {
    /// The common header naming this device, with `flags`.
    ///
    /// Default flags emit no `FLAGS` attribute at all, which is what
    /// `ethtool(8)` sends for a plain query.
    pub fn header(&self, flags: RequestFlags) -> Vec<HeaderAttr> {
        match self {
            Self::Name(name) => HeaderAttr::for_device(name, flags),
            Self::Index(index) => HeaderAttr::for_index(*index, flags),
        }
    }
}

impl fmt::Display for Device {
    /// The name, or `#<index>` for an index.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(name) => f.write_str(name),
            Self::Index(index) => write!(f, "#{index}"),
        }
    }
}

impl From<&str> for Device {
    fn from(name: &str) -> Self {
        Self::Name(name.to_string())
    }
}

impl From<String> for Device {
    fn from(name: String) -> Self {
        Self::Name(name)
    }
}

impl From<&String> for Device {
    fn from(name: &String) -> Self {
        Self::Name(name.clone())
    }
}

impl From<u32> for Device {
    fn from(index: u32) -> Self {
        Self::Index(index)
    }
}

impl From<&Device> for Device {
    fn from(device: &Device) -> Self {
        device.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_maps_to_dev_name() {
        assert_eq!(
            Device::from("enp2s0f0np0").header(RequestFlags::default()),
            vec![HeaderAttr::DevName("enp2s0f0np0".into())]
        );
    }

    #[test]
    fn index_maps_to_dev_index() {
        assert_eq!(
            Device::from(5).header(RequestFlags::default()),
            vec![HeaderAttr::DevIndex(5)]
        );
    }

    #[test]
    fn display() {
        assert_eq!(Device::from("eth0").to_string(), "eth0");
        assert_eq!(Device::from(7).to_string(), "#7");
    }

    /// Exactly one identifying attribute, of the right kind, and a `FLAGS`
    /// attribute exactly when some flag is set.
    #[test]
    fn header_mapping_property() {
        bolero::check!()
            .with_type::<(bool, String, u32, RequestFlags)>()
            .for_each(|(by_name, name, index, flags)| {
                let device = if *by_name {
                    Device::Name(name.clone())
                } else {
                    Device::Index(*index)
                };
                let header = device.header(*flags);
                let identity: Vec<_> = header
                    .iter()
                    .filter(|a| matches!(a, HeaderAttr::DevName(_) | HeaderAttr::DevIndex(_)))
                    .collect();
                match &device {
                    Device::Name(n) => assert_eq!(identity, vec![&HeaderAttr::DevName(n.clone())]),
                    Device::Index(i) => assert_eq!(identity, vec![&HeaderAttr::DevIndex(*i)]),
                }
                let sent: Vec<_> = header
                    .iter()
                    .filter_map(|a| match a {
                        HeaderAttr::Flags(f) => Some(*f),
                        _ => None,
                    })
                    .collect();
                if *flags == RequestFlags::default() {
                    assert!(sent.is_empty());
                } else {
                    assert_eq!(sent, vec![*flags]);
                }
            });
    }
}
