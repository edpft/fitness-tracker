//! Which device sent a reading that arrives without naming one (#427).
//!
//! **The Bike+ relays a heart rate and names no device.** Heart rate per power
//! zone is one series per device (#152), so each ride needs one, and this is
//! where it comes from: every device the record identifies, when the record
//! shows it in use, and what it sends to another device.
//!
//! **One entry per real-world device** (operator, 2026-10-13): *"We will need to
//! use better identifiers than the name."* A device is identified by what the
//! hardware reports — its serial number, or the ANT+ number it broadcasts by —
//! and can carry both.
//!
//! **Derived, never stated.** Every source of a ride's heart rate identifies
//! itself or can be placed by when it was active, so nothing here is the
//! operator's to say (operator, 2026-10-13), and the register re-derives from
//! what has landed every time it is asked for (§ 7).

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    num::NonZeroU16,
};

use jiff::civil::Date;

use crate::{newtype::string_name, normalised::StartedAt};

/// The longest a device's name may be. A line on a page, not a rule about naming.
pub const MAX_NAME: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidDeviceName {
    #[error("a device's name must not be empty")]
    Empty,
    #[error("a device's name must be at most {MAX_NAME} characters, and this is {length}")]
    TooLong { length: usize },
    #[error("a device's name must be one line of printable text")]
    NotPrintable,
}

/// What a device is called: *Forerunner 165*.
///
/// **It identifies nothing.** Two straps of one model share it; identity is
/// [`DeviceIdentifier`]'s.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceName(String);

impl TryFrom<String> for DeviceName {
    type Error = InvalidDeviceName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(InvalidDeviceName::Empty);
        }
        let length = trimmed.chars().count();
        if length > MAX_NAME {
            return Err(InvalidDeviceName::TooLong { length });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(InvalidDeviceName::NotPrintable);
        }
        Ok(Self(trimmed.to_owned()))
    }
}

string_name!(DeviceName, InvalidDeviceName);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidSerialNumber {
    #[error("a serial number must not be empty")]
    Empty,
    #[error("a serial number is one word of printable text, without '/'")]
    NotOneWord,
}

/// The number a device reports itself by.
///
/// **Text, not a number**, because a device's maker decides its shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SerialNumber(String);

impl TryFrom<String> for SerialNumber {
    type Error = InvalidSerialNumber;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(InvalidSerialNumber::Empty);
        }
        if value.chars().any(|character| {
            character == '/' || character.is_whitespace() || character.is_control()
        }) {
            return Err(InvalidSerialNumber::NotOneWord);
        }
        Ok(Self(value))
    }
}

string_name!(SerialNumber, InvalidSerialNumber);

/// The number a device broadcasts by over ANT+.
///
/// **The low sixteen bits of its serial number**, which is how a receiver that
/// only hears the broadcast names it. Checked on 2026-10-13 against three
/// receivers: Peloton's paired monitor 3629 is the Forerunner 165 (serial
/// 3483897389); the Wahoo app names the TICKR 10341 (serial 3339528293), and
/// the TICKR calls itself "TICKR 2865", which is 10341 in hex; and the Edge 530
/// lists the same TICKR as 10341.
///
/// **Sixteen bits, not twenty.** ANT+ lets a device extend its number with four
/// more bits, and Garmin's do: the Edge lists the Forerunner 165 as 527917, the
/// low twenty bits of its serial. Peloton and the Wahoo app show the sixteen,
/// and the sixteen is what every receiver has in common.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AntDeviceNumber(NonZeroU16);

impl AntDeviceNumber {
    pub const fn new(number: NonZeroU16) -> Self {
        Self(number)
    }

    /// The number a device with this serial broadcasts by, where it has one:
    /// a serial whose low sixteen bits are zero names no ANT+ device.
    #[must_use]
    pub fn of_serial(serial: u32) -> Option<Self> {
        let low = u16::try_from(serial & 0xFFFF).ok()?;
        NonZeroU16::new(low).map(Self)
    }

    pub const fn get(self) -> NonZeroU16 {
        self.0
    }
}

/// What one real-world device is told apart by.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeviceIdentifier {
    /// The serial number the device reports.
    Serial(SerialNumber),
    /// The number it broadcasts by over ANT+.
    Ant(AntDeviceNumber),
}

impl fmt::Display for DeviceIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serial(serial) => write!(f, "serial/{serial}"),
            Self::Ant(number) => write!(f, "ant/{}", number.0),
        }
    }
}

/// What a device measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Measurand {
    HeartRate,
    Power,
    Cadence,
    Speed,
}

impl fmt::Display for Measurand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::HeartRate => "heart rate",
            Self::Power => "power",
            Self::Cadence => "cadence",
            Self::Speed => "speed",
        })
    }
}

/// Where a record placed a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// It wrote the recording.
    Recorder,
    /// It was paired to whatever wrote it.
    Paired,
}

/// What one record says about one device it used.
///
/// **A source's account, translated** (§ 9): the adapter that read the record
/// names the device in this crate's terms, and nothing here knows which
/// source that was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
    pub device: DeviceIdentifier,
    /// Every other identifier the same record states for it: a serial's ANT+
    /// number. Two sightings sharing any identifier are one device.
    pub also: Vec<DeviceIdentifier>,
    pub name: DeviceName,
    pub role: Role,
    /// What it supplied to this recording. A watch paired with a strap
    /// supplied no heart rate, though it could have.
    pub measured: BTreeSet<Measurand>,
    /// What it can send to another device. A strap sends the heart rate it
    /// measures; a watch that broadcasts sends it whether or not anything
    /// recorded it doing so.
    pub sent: BTreeSet<Measurand>,
    pub at: StartedAt,
    /// Whose record this is: *Garmin*, *Peloton*.
    pub source: &'static str,
    /// Whether it is worn on the wrist, so that the days it was worn are the
    /// days it was in use.
    pub worn: bool,
}

/// Which days a wrist device was worn, as the account's daily heart rate says.
///
/// **A day the account holds and shows no heart rate for is a day nothing was
/// worn**: the operator found no all-day heart rate from 2023-10-08 to
/// 2024-07-16, and the Vivoactive 3 recorded nothing in that time either. A
/// day the account does not hold says nothing, and the device's span stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wear(BTreeMap<Date, bool>);

impl Wear {
    #[must_use]
    pub const fn new(days: BTreeMap<Date, bool>) -> Self {
        Self(days)
    }

    /// Whether something was worn on `date`, where the account says.
    #[must_use]
    pub fn on(&self, date: Date) -> Option<bool> {
        self.0.get(&date).copied()
    }
}

/// One real-world device, and when the record shows it in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    identifiers: Vec<DeviceIdentifier>,
    name: DeviceName,
    measures: BTreeSet<Measurand>,
    sends: BTreeSet<Measurand>,
    first_used: Date,
    last_used: Date,
    worn: bool,
}

impl Device {
    /// Every identifier it reports, the first it was seen by first.
    pub fn identifiers(&self) -> &[DeviceIdentifier] {
        &self.identifiers
    }

    pub fn is(&self, identifier: &DeviceIdentifier) -> bool {
        self.identifiers.contains(identifier)
    }

    pub const fn name(&self) -> &DeviceName {
        &self.name
    }

    pub const fn measures(&self) -> &BTreeSet<Measurand> {
        &self.measures
    }

    /// What it sends to another device, which is what makes it a candidate
    /// for a reading that arrives without naming its device.
    pub const fn sends(&self) -> &BTreeSet<Measurand> {
        &self.sends
    }

    /// The first record of it in use.
    pub const fn first_used(&self) -> Date {
        self.first_used
    }

    /// The last record of it in use, which is the earliest it could have
    /// stopped being used.
    pub const fn last_used(&self) -> Date {
        self.last_used
    }

    /// Whether the record shows it in use on `date`: inside its span, and for
    /// a device worn on the wrist, worn that day where the account says.
    #[must_use]
    pub fn in_use_on(&self, date: Date, wear: &Wear) -> bool {
        let spanned = self.first_used <= date && date <= self.last_used;
        spanned && (!self.worn || wear.on(date).unwrap_or(true))
    }

    /// Another account of this device: every identifier, everything either
    /// measured or sent, and the widest period the two give.
    fn absorb(&mut self, other: Self, name_from_other: bool) {
        for identifier in other.identifiers {
            if !self.is(&identifier) {
                self.identifiers.push(identifier);
            }
        }
        self.measures.extend(other.measures);
        self.sends.extend(other.sends);
        self.first_used = self.first_used.min(other.first_used);
        self.last_used = self.last_used.max(other.last_used);
        self.worn |= other.worn;
        if name_from_other {
            self.name = other.name;
        }
    }
}

/// Every device the record identifies, ordered by first use.
///
/// Deterministic (§ 9). **Sightings sharing any identifier are one device**, so
/// a watch's serial and the ANT+ number the Bike+'s paired monitor names join
/// into one entry. **What a device calls itself when it wrote a recording is
/// its name**, rather than what something it was paired to called it.
#[must_use]
pub fn register(sightings: &[Sighting], wear: Wear) -> Register {
    Register {
        devices: devices(sightings),
        wear,
    }
}

/// Every device the record identifies, and which days a wrist device was worn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Register {
    devices: Vec<Device>,
    wear: Wear,
}

impl Register {
    /// Every device, ordered by first use.
    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    /// Which device sent `what` on `date`: see [`sent_by`].
    #[must_use]
    pub fn sent_by(&self, what: Measurand, date: Date) -> InUse<'_> {
        sent_by(&self.devices, &self.wear, what, date)
    }
}

fn devices(sightings: &[Sighting]) -> Vec<Device> {
    let mut devices: Vec<(Device, bool)> = Vec::new();
    for sighting in sightings {
        let date = sighting.at.wall_clock().date();
        let identifiers: Vec<DeviceIdentifier> = std::iter::once(&sighting.device)
            .chain(&sighting.also)
            .cloned()
            .collect();
        let wrote_it = sighting.role == Role::Recorder;
        let mut sighted = (
            Device {
                identifiers: identifiers.clone(),
                name: sighting.name.clone(),
                measures: sighting.measured.clone(),
                sends: sighting.sent.clone(),
                first_used: date,
                last_used: date,
                worn: sighting.worn,
            },
            wrote_it,
        );
        let mut index = 0;
        while index < devices.len() {
            let shares = devices
                .get(index)
                .is_some_and(|(held, _)| identifiers.iter().any(|identifier| held.is(identifier)));
            if shares {
                let (mut held, held_as_recorder) = devices.remove(index);
                let (other, other_as_recorder) = sighted;
                held.absorb(other, other_as_recorder && !held_as_recorder);
                sighted = (held, held_as_recorder || other_as_recorder);
            } else {
                index = index.saturating_add(1);
            }
        }
        devices.push(sighted);
    }
    let mut devices: Vec<Device> = devices.into_iter().map(|(device, _)| device).collect();
    devices.sort_by(|one, other| {
        (one.first_used, &one.identifiers).cmp(&(other.first_used, &other.identifiers))
    });
    devices
}

/// Which device sent a reading on a date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InUse<'a> {
    /// The one device in use then that sends it.
    One(&'a Device),
    /// More than one was, and nothing records which.
    Several(Vec<&'a Device>),
    /// None was.
    Unknown,
}

/// Which device sent `what` on `date` to whatever recorded it, where the
/// recording does not say.
///
/// **Only a device that sends what it measures is a candidate.** The Bike+
/// relays a heart rate and names no device; a strap or a watch broadcasting
/// could have sent it, and a device that measures only for itself could not
/// (operator, 2026-10-13).
///
/// **A watch is a candidate only on a day it was worn**, where the account's
/// daily heart rate says.
///
/// **One, or it is not known.** Two candidates in use is two candidates, and
/// choosing between them would be a guess (§ 37).
#[must_use]
pub fn sent_by<'a>(devices: &'a [Device], wear: &Wear, what: Measurand, date: Date) -> InUse<'a> {
    let candidates: Vec<&Device> = devices
        .iter()
        .filter(|device| device.sends.contains(&what) && device.in_use_on(date, wear))
        .collect();
    match candidates.as_slice() {
        [] => InUse::Unknown,
        [one] => InUse::One(one),
        _ => InUse::Several(candidates),
    }
}
