use crate::SeatName;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DeviceKind {
    Drm,
    Evdev,
    Hidraw,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SeatDevice {
    pub syspath: PathBuf,
    pub devnode: Option<PathBuf>,
    pub devnum: Option<u64>,
    pub seat: SeatName,
    pub kind: DeviceKind,
    pub uaccess: bool,
    pub master_of_seat: bool,
}

impl SeatDevice {
    pub fn brokerable(&self) -> bool {
        matches!(
            self.kind,
            DeviceKind::Drm | DeviceKind::Evdev | DeviceKind::Hidraw
        ) && self.devnode.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Filter {
    All,
    Brokerable,
    Uaccess,
}

impl Filter {
    fn matches(self, device: &SeatDevice) -> bool {
        match self {
            Self::All => true,
            Self::Brokerable => device.brokerable(),
            Self::Uaccess => device.uaccess,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    DeviceAdded(SeatDevice),
    DeviceRemoved(SeatDevice),
    DeviceMoved { device: SeatDevice, from: SeatName },
    SeatAdded(SeatName),
    SeatRemoved(SeatName),
}

/// Pure cache and transition logic. The real libudev context feeds this model;
/// tests and consumers can exercise it without libudev, root, or hardware.
#[derive(Clone, Debug, Default)]
pub struct Model {
    devices: BTreeMap<PathBuf, SeatDevice>,
}

impl Model {
    pub fn seed(&mut self, device: SeatDevice) {
        self.devices.insert(device.syspath.clone(), device);
    }

    pub fn add_or_change(&mut self, device: SeatDevice) -> Vec<Event> {
        let before = self.seats();
        let old = self.devices.insert(device.syspath.clone(), device.clone());
        let after = self.seats();
        let mut events = Vec::new();

        match old {
            None => {
                push_added_seats(&before, &after, &mut events);
                events.push(Event::DeviceAdded(device));
            }
            Some(old) if old.seat != device.seat => {
                events.push(Event::DeviceMoved {
                    device,
                    from: old.seat,
                });
                push_removed_seats(&before, &after, &mut events);
                push_added_seats(&before, &after, &mut events);
            }
            Some(_) => {
                // Property-only changes on one seat update the cache but are
                // not lifecycle changes.
            }
        }
        events
    }

    pub fn remove(&mut self, syspath: &Path) -> Vec<Event> {
        let before = self.seats();
        let Some(old) = self.devices.remove(syspath) else {
            return Vec::new();
        };
        let after = self.seats();
        let mut events = vec![Event::DeviceRemoved(old)];
        push_removed_seats(&before, &after, &mut events);
        events
    }

    pub fn seats(&self) -> BTreeSet<SeatName> {
        self.devices
            .values()
            .filter(|device| device.master_of_seat)
            .map(|device| device.seat.clone())
            .collect()
    }

    pub fn devices(&self, seat: &SeatName, filter: Filter) -> Vec<SeatDevice> {
        self.devices
            .values()
            .filter(|device| &device.seat == seat && filter.matches(device))
            .cloned()
            .collect()
    }

    pub fn by_syspath(&self, syspath: &Path) -> Option<SeatDevice> {
        self.devices.get(syspath).cloned()
    }

    pub fn by_devnode(&self, devnode: &Path) -> Option<SeatDevice> {
        self.devices
            .values()
            .find(|device| device.devnode.as_deref() == Some(devnode))
            .cloned()
    }

    pub fn by_devnum(&self, devnum: u64) -> Option<SeatDevice> {
        self.devices
            .values()
            .find(|device| device.devnum == Some(devnum))
            .cloned()
    }
}

fn push_added_seats(
    before: &BTreeSet<SeatName>,
    after: &BTreeSet<SeatName>,
    events: &mut Vec<Event>,
) {
    events.extend(after.difference(before).cloned().map(Event::SeatAdded));
}

fn push_removed_seats(
    before: &BTreeSet<SeatName>,
    after: &BTreeSet<SeatName>,
    events: &mut Vec<Event>,
) {
    events.extend(before.difference(after).cloned().map(Event::SeatRemoved));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(path: &str, seat: &str, master: bool) -> SeatDevice {
        SeatDevice {
            syspath: path.into(),
            devnode: Some(format!("/dev/{}", path.trim_start_matches('/')).into()),
            devnum: None,
            seat: SeatName::parse(seat).unwrap(),
            kind: DeviceKind::Drm,
            uaccess: false,
            master_of_seat: master,
        }
    }

    #[test]
    fn first_and_last_master_define_seat_lifetime() {
        let mut model = Model::default();
        let first = device("/gpu0", "seat0", true);
        assert_eq!(
            model.add_or_change(first.clone()),
            vec![
                Event::SeatAdded(SeatName::seat0()),
                Event::DeviceAdded(first.clone())
            ]
        );
        let second = device("/gpu1", "seat0", true);
        assert_eq!(
            model.add_or_change(second.clone()),
            vec![Event::DeviceAdded(second)]
        );
        assert_eq!(model.remove(Path::new("/gpu0")).len(), 1);
        assert_eq!(
            model.remove(Path::new("/gpu1")),
            vec![
                Event::DeviceRemoved(device("/gpu1", "seat0", true)),
                Event::SeatRemoved(SeatName::seat0())
            ]
        );
    }

    #[test]
    fn move_reports_old_seat_and_both_lifecycles() {
        let mut model = Model::default();
        model.seed(device("/gpu", "seat0", true));
        let moved = device("/gpu", "seat1", true);
        assert_eq!(
            model.add_or_change(moved.clone()),
            vec![
                Event::DeviceMoved {
                    device: moved,
                    from: SeatName::seat0()
                },
                Event::SeatRemoved(SeatName::seat0()),
                Event::SeatAdded(SeatName::parse("seat1").unwrap())
            ]
        );
    }

    #[test]
    fn same_seat_change_is_swallowed_but_updates_cache() {
        let mut model = Model::default();
        let mut old = device("/event0", "seat0", false);
        old.kind = DeviceKind::Evdev;
        model.seed(old.clone());
        let mut changed = old;
        changed.uaccess = true;
        assert!(model.add_or_change(changed.clone()).is_empty());
        assert_eq!(
            model.devices(&SeatName::seat0(), Filter::Uaccess),
            vec![changed]
        );
    }

    #[test]
    fn duplicate_remove_is_ignored() {
        let mut model = Model::default();
        assert!(model.remove(Path::new("/missing")).is_empty());
    }
}
