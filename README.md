# ethtool2

Query Linux network devices through the `ethtool` generic netlink family:
link modes and state, pause, offload features, private flags, FEC, the
standardised statistics, string sets, and pluggable module memory.

```rust
let (connection, handle) = ethtool2::new_connection()?;
tokio::spawn(connection);

let modes = handle.link_modes("eth0").await?;
println!("{:?} Mb/s over {:?} lanes", modes.speed, modes.lanes);

let stats = handle.stats("eth0", &ethtool2::STANDARD_STATS_GROUPS).await?;
for group in &stats.groups {
    for counter in &group.counters {
        println!("{:?}-{:?}: {}", group.label(), counter.name, counter.value);
    }
}
```

## Relationship to `netlink-packet-ethtool`

This crate is the async handle; the wire format lives in
[`netlink-packet-ethtool`](https://github.com/githedgehog/netlink-packet-ethtool),
re-exported as `packet_ethtool`. The split is the one `devlink` and
`netlink-packet-devlink` use:

| crate | does |
| --- | --- |
| `netlink-packet-ethtool` | message and attribute types, encode/decode, request builders |
| this crate | the socket, the request/reply exchange, typed views, errors |

The packet crate's module docs are where the wire-format traps are written
down -- statistics that arrive only when asked for, two bitset encodings
chosen by a request flag, counters whose identity is their attribute number.
This crate handles those so callers do not have to, and is the place to look
when a typed method returns something surprising.

## Operations

Every method is a `GET`, named after what it returns:

| method | `ethtool(8)` |
| --- | --- |
| `link_info` | `ethtool <dev>` (`Port:`, `PHYAD:`, `Transceiver:`) |
| `link_modes` | `ethtool <dev>` (speed, duplex, lanes, autoneg, link modes) |
| `link_state` | `ethtool <dev>` (`Link detected:`) |
| `pause` / `pause_with_stats` | `ethtool -a` / `ethtool -I -a` |
| `features` | `ethtool -k` |
| `private_flags` | `ethtool --show-priv-flags` |
| `fec` / `fec_with_stats` | `ethtool --show-fec` / `ethtool -I --show-fec` |
| `stats` | `ethtool -S --groups eth-phy eth-mac eth-ctrl rmon` |
| `string_sets` / `global_string_sets` | (what `ethtool -S` resolves names with) |
| `module_eeprom` | `ethtool -m <dev> page .. bank .. offset .. length .. raw on` |

Each returns a **lossless view**: named fields for what the crate
understands, and `attributes`, every attribute the kernel sent, in order.

Devices are named by `Device`: a `&str` or `String` is a name, a `u32` an
interface index. An index survives a rename and is what a caller learns from
rtnetlink. Either is looked up in the netlink socket's network namespace.

For anything else there are two escape hatches:

* `Handle::request` sends any message built with the packet crate and
  returns the decoded replies;
* `Handle::raw` sends a command number and raw attribute bytes and returns
  the replies undecoded -- for settling an encoding when the decoder and the
  kernel disagree. `examples/probe_raw.rs` walks such a reply.

## Module EEPROM needs `CAP_NET_ADMIN`

Unlike every other operation here, `MODULE_EEPROM_GET` is an admin-only
operation in the kernel: without `CAP_NET_ADMIN` in the device's user
namespace it fails with `Error::PermissionDenied`. Each read is a transaction
on the module's two-wire bus, at most half a page; `EepromWindow` can only
describe a read the kernel will accept. The bytes are returned as read --
interpreting them is for the module's memory map (SFF-8472, SFF-8636, CMIS).

## Errors

`Error` keeps its sources typed. The errnos callers act on have variants of
their own:

| errno | variant | typical cause |
| --- | --- | --- |
| `EOPNOTSUPP` | `Unsupported` | the driver does not implement the operation |
| `EPERM` | `PermissionDenied` | a module EEPROM read without `CAP_NET_ADMIN` |
| `ENODEV` | `NoSuchDevice` | no device by that name or index in the socket's namespace |

Every other kernel error is `Kernel(ErrorMessage)`, unchanged.

## Verified

Checked read-only against `ethtool` 7.1 on a BlueField-3 (mlx5, kernel 6.18)
with a CMIS QSFP112 module:

* link modes: the same 54 supported and 54 advertised modes, speed, lanes,
  duplex, autoneg, link detected;
* features: all 60 agree on state and `[fixed]`, under the kernel's names
  (`ethtool -k` shows eight of them under older aliases, e.g. `rx-checksum`
  as `rx-checksumming`);
* private flags, pause with and without statistics, FEC with and without
  statistics: identical;
* standard statistics: output identical to `ethtool -S --groups ...`,
  30 lines;
* module EEPROM: pages 0 and 1 byte-identical to `ethtool -m ... raw on`,
  addressed by name and by ifindex; the lower half differs only in the live
  supply-voltage byte, which differs between consecutive reads of either
  tool.

## Examples

`cargo run --example <name> -- <dev>`, for `link`, `features`, `pause`,
`fec`, `stats`, `module_eeprom`, and the probes `probe_raw` and
`probe_stats`. `pause` and `fec` take `--stats`; `module_eeprom` takes
`<dev|ifindex> [page] [bank] [lower]` and needs `CAP_NET_ADMIN`.

## Testing

`cargo test`: unit tests for every view built from synthetic attribute lists,
for request construction (flags, device addressing) and for error
classification, with `bolero` property tests where there is a property to
state -- every view keeps its attributes whole, every request names the
device it was given.
