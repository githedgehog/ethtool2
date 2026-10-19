// SPDX-License-Identifier: MIT

use genetlink::GenetlinkError;
use netlink_packet_core::{DecodeError, ErrorMessage};
use netlink_packet_ethtool::EthtoolCmd;
use thiserror::Error;

use crate::Device;

/// `EPERM`, as the kernel reports it (negated) in `NLMSG_ERROR`.
const EPERM: i32 = 1;
/// `ENODEV`.
const ENODEV: i32 = 19;
/// `EOPNOTSUPP`.
const EOPNOTSUPP: i32 = 95;

/// Why a request failed.
///
/// The three errnos a caller is likely to act on have variants of their own;
/// every other kernel error is kept whole in [`Error::Kernel`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The driver does not implement this operation (`EOPNOTSUPP`).
    ///
    /// Ordinary rather than a fault: most drivers implement only part of
    /// ethtool, and some answer only while the link is up or a module is
    /// plugged in.
    #[error("the device does not support this operation")]
    Unsupported,

    /// The caller lacks a capability the operation needs (`EPERM`).
    ///
    /// Module EEPROM reads are the common case: unlike most `GET`s they
    /// need `CAP_NET_ADMIN` in the device's user namespace.
    #[error("operation not permitted (it may need CAP_NET_ADMIN)")]
    PermissionDenied,

    /// No network device has this name or index (`ENODEV`), in the netlink
    /// socket's network namespace.
    ///
    /// `device` is filled in by the typed methods of [`crate::Handle`]; a
    /// failure through [`crate::Handle::request`] cannot say which device it
    /// was about.
    #[error("no such network device{}", device.as_ref().map(|d| format!(": {d}")).unwrap_or_default())]
    NoSuchDevice { device: Option<Device> },

    /// Any other error the kernel answered with.
    #[error("the kernel refused the request: {0}")]
    Kernel(ErrorMessage),

    /// The request could not be sent, or the ethtool family could not be
    /// resolved -- for instance on a kernel built without
    /// `CONFIG_ETHTOOL_NETLINK`.
    #[error("failed to send an ethtool request")]
    Request(#[source] GenetlinkError),

    /// A reply arrived but did not decode.
    #[error("failed to decode an ethtool reply")]
    Decode(#[source] DecodeError),

    /// The kernel acknowledged a `GET` without sending the reply it should
    /// have.
    #[error("the kernel sent no {0:?} reply")]
    NoReply(EthtoolCmd),
}

impl Error {
    /// Classify an `NLMSG_ERROR` carrying a non-zero code.
    pub(crate) fn from_kernel(message: ErrorMessage) -> Self {
        match -message.raw_code() {
            EOPNOTSUPP => Self::Unsupported,
            EPERM => Self::PermissionDenied,
            ENODEV => Self::NoSuchDevice { device: None },
            _ => Self::Kernel(message),
        }
    }

    /// Name the device a [`Error::NoSuchDevice`] was about.
    pub(crate) fn about(self, device: &Device) -> Self {
        match self {
            Self::NoSuchDevice { device: None } => Self::NoSuchDevice {
                device: Some(device.clone()),
            },
            other => other,
        }
    }

    /// The errno behind this error, if the kernel sent one.
    pub fn raw_os_error(&self) -> Option<i32> {
        match self {
            Self::Unsupported => Some(EOPNOTSUPP),
            Self::PermissionDenied => Some(EPERM),
            Self::NoSuchDevice { .. } => Some(ENODEV),
            Self::Kernel(message) => Some(-message.raw_code()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroI32;

    use super::*;

    fn kernel(errno: i32) -> ErrorMessage {
        let mut message = ErrorMessage::default();
        message.code = NonZeroI32::new(-errno);
        message
    }

    #[test]
    fn classifies_errnos_callers_act_on() {
        assert!(matches!(Error::from_kernel(kernel(95)), Error::Unsupported));
        assert!(matches!(
            Error::from_kernel(kernel(1)),
            Error::PermissionDenied
        ));
        assert!(matches!(
            Error::from_kernel(kernel(19)),
            Error::NoSuchDevice { device: None }
        ));
    }

    #[test]
    fn keeps_other_errnos_whole() {
        // EINVAL and EIO are common from ethtool and carry no special meaning.
        for errno in [22, 5, 16] {
            let error = Error::from_kernel(kernel(errno));
            assert!(matches!(&error, Error::Kernel(m) if m.raw_code() == -errno));
            assert_eq!(error.raw_os_error(), Some(errno));
        }
    }

    #[test]
    fn raw_os_error_round_trips() {
        bolero::check!().with_type::<u16>().for_each(|errno| {
            let errno = i32::from(*errno) + 1;
            assert_eq!(
                Error::from_kernel(kernel(errno)).raw_os_error(),
                Some(errno)
            );
        });
    }

    #[test]
    fn about_names_only_a_missing_device() {
        let device = Device::from("eth9");
        let error = Error::from_kernel(kernel(19)).about(&device);
        assert_eq!(error.to_string(), "no such network device: eth9");
        assert!(matches!(
            Error::from_kernel(kernel(95)).about(&device),
            Error::Unsupported
        ));
    }
}
