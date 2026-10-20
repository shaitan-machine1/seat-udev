# seat-udev

Shared udev seat discovery and persistent assignment for the Phase 1 login
stack. This repository contains:

- `seat-udev`: a Rust library with a libudev-free semantic API, a
  monitor-before-enumerate cache, pure transition model, and optional
  assignment support;
- `seatctl`: the root CLI for inspecting seats and atomically adding/removing
  `72-seat-<ID_FOR_SEAT>.rules` assignments.

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

## seatctl

```sh
seatctl status
sudo seatctl add /dev/dri/card1 seat1
sudo seatctl remove /dev/dri/card1
sudo seatctl flush
```

Assignment resolves the nearest `seat`-tagged ancestor with `ID_FOR_SEAT`,
writes the rule atomically under `/etc/udev/rules.d`, reloads udevd, opens a
monitor, writes one correlated `change <uuid>` synthetic event to the complete
device group, and waits for every matching `SYNTH_UUID`. It deliberately does
not use global `udevadm settle`.
