# seat-udev

Shared udev seat discovery and persistent assignment for the Phase 1 login
stack. This repository contains:

- `seat-udev`: a Rust library with a libudev-free semantic API, a
  monitor-before-enumerate cache, pure transition model, and optional
  assignment support.

The separate unified `loginctl` client compiles this library directly for seat
inspection and atomic `72-seat-<ID_FOR_SEAT>.rules` assignment. This repository
intentionally ships no component-specific CLI.

`multiseatd` consumes brokerable DRM/evdev/hidraw devices. `aclad` consumes
`uaccess` devices. Both pin this repository by an exact git tag.

## Build and test

```sh
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Tests use the pure model and a fake assignment backend. The small live test
opens the local libudev database and queries `/dev/null`; it needs no root or
graphical hardware.

## Assignment

Assignment through the library resolves the nearest `seat`-tagged ancestor with `ID_FOR_SEAT`,
writes the rule atomically under `/etc/udev/rules.d`, reloads udevd, opens a
monitor, writes one correlated `change <uuid>` synthetic event to the complete
device group, and waits for every matching `SYNTH_UUID`. It deliberately does
not use global `udevadm settle`.

## Udev rules

`dist/udev/71-seat.rules` and `dist/udev/73-seat-late.rules` must be installed
into `/usr/lib/udev/rules.d`. They are this stack's replacements for the files
systemd and elogind ship, and they are what creates the whole seat model:

- `seat` marks a device as a seat member and `master-of-seat` marks the
  devices that make a seat usable at all;
- `ID_FOR_SEAT` is the stable name the assignment API writes rules against;
- `ID_SEAT` is the resolved seat, inherited from the nearest assigned
  ancestor, with no property meaning `seat0`.

Without them nothing is tagged, every consumer sees an empty device set, and
`multiseatd` publishes no seat at all. Unlike systemd's `73-seat-late.rules`,
ours never runs udev's `uaccess` builtin: `aclad` owns device ACLs.

After installing or changing the files, `udevadm control --reload` followed by
a `udevadm trigger` (or a reboot) is needed for devices that already exist.
