//! The devices the operator uses, and when each was in use (#427).
//!
//! **One entry per real-world device**, never one per way a recording names it
//! (operator, 2026-10-13): *"We will need to use better identifiers than the
//! name."* A device is identified by what the hardware reports about itself —
//! its serial number — and can carry several, because one strap reports one
//! number over ANT+ and another over Bluetooth.
//!
//! **What the record shows is the baseline; what the operator states takes
//! precedence over it.** The operator: *"We should treat the information we've
//! recovered from Garmin and other sources as a base line, I should be able to
//! update that."* So only his statements are stored. The baseline is derived
//! from what has landed every time it is asked for, and moves forward as new
//! recordings arrive.
//!
//! **It is a fact about the world**, as the plates at the gym are, and it is
//! consulted to interpret observations: which series a heart rate belongs to
//! (§ 6) depends on which device measured it. So a statement is kept when a
//! later one supersedes it (§ 13).

use std::{
    collections::BTreeSet,
    fmt,
    num::{NonZeroU16, NonZeroU64},
};

use jiff::{Timestamp, civil::Date};

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

/// What a device is called: *Forerunner 165*, *Wahoo TICKR*.
///
/// **It identifies nothing.** Two straps of one model share it, and the
/// operator can rename one; identity is [`DeviceIdentifier`]'s.
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
/// **Text, not a number**, because a device's maker decides its shape: Garmin's
/// are 32-bit integers, and a scale's may be a hardware address.
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

/// Which platform issued an identifier: *Withings*.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Platform(String);

impl TryFrom<String> for Platform {
    type Error = InvalidSerialNumber;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        SerialNumber::try_from(value).map(|word| Self(word.0))
    }
}

string_name!(Platform, InvalidSerialNumber);

/// The id a platform gave a device that reports no number of its own to it.
///
/// **Not a serial number**, and not passed off as one: Withings serves a hash
/// of its own for each scale, which is as stable as a serial and names the
/// same scale every time, but which is Withings' rather than printed on the
/// hardware. The platform travels with it, because two platforms' ids share
/// no namespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IssuedId {
    pub platform: Platform,
    pub id: SerialNumber,
}

/// A device the operator added, which no recording names.
///
/// Numbered by the store that holds the statement adding it: there is no
/// hardware number to use, and a name identifies nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatedDevice(NonZeroU64);

impl StatedDevice {
    pub const fn new(number: NonZeroU64) -> Self {
        Self(number)
    }

    pub const fn get(self) -> NonZeroU64 {
        self.0
    }
}

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
    /// The id the platform it reports to gave it.
    Issued(IssuedId),
    /// The number it broadcasts by over ANT+.
    Ant(AntDeviceNumber),
    /// A device the operator added himself.
    Stated(StatedDevice),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{value:?} does not identify a device: it is serial/<number>, issued/<platform>/<id>, ant/<number> or stated/<number>"
)]
pub struct InvalidDeviceIdentifier {
    value: String,
}

impl TryFrom<String> for DeviceIdentifier {
    type Error = InvalidDeviceIdentifier;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let parsed = match value.split_once('/') {
            Some(("serial", serial)) => SerialNumber::try_from(serial).ok().map(Self::Serial),
            Some(("issued", issued)) => issued.split_once('/').and_then(|(platform, id)| {
                Some(Self::Issued(IssuedId {
                    platform: Platform::try_from(platform).ok()?,
                    id: SerialNumber::try_from(id).ok()?,
                }))
            }),
            Some(("ant", number)) => number
                .parse::<NonZeroU16>()
                .ok()
                .map(|number| Self::Ant(AntDeviceNumber(number))),
            Some(("stated", number)) => number
                .parse::<NonZeroU64>()
                .ok()
                .map(|number| Self::Stated(StatedDevice(number))),
            _ => None,
        };
        parsed.ok_or(InvalidDeviceIdentifier { value })
    }
}

crate::newtype::from_str_via_string!(DeviceIdentifier, InvalidDeviceIdentifier);

impl fmt::Display for DeviceIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serial(serial) => write!(f, "serial/{serial}"),
            Self::Issued(issued) => write!(f, "issued/{}/{}", issued.platform, issued.id),
            Self::Ant(number) => write!(f, "ant/{}", number.0),
            Self::Stated(stated) => write!(f, "stated/{}", stated.0),
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
    BodyMass,
    BodyComposition,
}

impl Measurand {
    pub const ALL: [Self; 6] = [
        Self::HeartRate,
        Self::Power,
        Self::Cadence,
        Self::Speed,
        Self::BodyMass,
        Self::BodyComposition,
    ];

    /// The stable key. Persisted.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HeartRate => "heart-rate",
            Self::Power => "power",
            Self::Cadence => "cadence",
            Self::Speed => "speed",
            Self::BodyMass => "body-mass",
            Self::BodyComposition => "body-composition",
        }
    }

    /// How a page or a terminal says it.
    pub const fn called(self) -> &'static str {
        match self {
            Self::HeartRate => "heart rate",
            Self::Power => "power",
            Self::Cadence => "cadence",
            Self::Speed => "speed",
            Self::BodyMass => "body mass",
            Self::BodyComposition => "body composition",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not something a device measures")]
pub struct UnknownMeasurand {
    value: String,
}

impl TryFrom<String> for Measurand {
    type Error = UnknownMeasurand;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|measurand| measurand.as_str() == value)
            .ok_or(UnknownMeasurand { value })
    }
}

crate::newtype::from_str_via_string!(Measurand, UnknownMeasurand);

impl fmt::Display for Measurand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.called())
    }
}

/// When a device was last used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LastUsed {
    On(Date),
    /// The operator says he still uses it. No recording can say this: a device
    /// paired to the Bike+ records nothing anywhere, so its last recording is
    /// only the earliest it could have stopped.
    StillInUse,
}

impl LastUsed {
    /// Whether a device last used so was still in use on `date`.
    #[must_use]
    pub fn covers(self, date: Date) -> bool {
        match self {
            Self::On(last) => date <= last,
            Self::StillInUse => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is neither a date nor \"still-in-use\"")]
pub struct InvalidLastUsed {
    value: String,
}

impl TryFrom<String> for LastUsed {
    type Error = InvalidLastUsed;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value == "still-in-use" {
            return Ok(Self::StillInUse);
        }
        value
            .parse::<Date>()
            .map(Self::On)
            .map_err(|_| InvalidLastUsed { value })
    }
}

crate::newtype::from_str_via_string!(LastUsed, InvalidLastUsed);

impl fmt::Display for LastUsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::On(date) => write!(f, "{date}"),
            Self::StillInUse => f.write_str("still in use"),
        }
    }
}

/// Where a recording placed a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// It wrote the recording.
    Recorder,
    /// It was paired to whatever wrote it.
    Paired,
}

/// What one recording says about one device it used.
///
/// **A source's account, translated** (§ 9): the adapter that read the
/// recording names the device in this crate's terms and says what it measured
/// in it, and nothing here knows which source that was.
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
    /// What it sent to the device that wrote the recording. A strap paired
    /// to a watch sent the heart rate it measured; the watch sent nothing.
    pub sent: BTreeSet<Measurand>,
    pub at: StartedAt,
    /// Whose recordings these are, as the register says it: *Garmin*.
    pub source: &'static str,
}

/// A device the operator adds, which no recording names.
///
/// Its period is ordered by construction (§ 24).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedDevice {
    name: DeviceName,
    measures: BTreeSet<Measurand>,
    first_used: Date,
    last_used: LastUsed,
}

impl AddedDevice {
    /// # Errors
    ///
    /// [`Contradiction::LastBeforeFirst`] if `last_used` precedes `first_used`.
    pub fn new(
        name: DeviceName,
        measures: BTreeSet<Measurand>,
        first_used: Date,
        last_used: LastUsed,
    ) -> Result<Self, Contradiction> {
        if !last_used.covers(first_used) {
            return Err(Contradiction::LastBeforeFirst {
                first: first_used,
                last: last_used,
            });
        }
        Ok(Self {
            name,
            measures,
            first_used,
            last_used,
        })
    }

    pub const fn name(&self) -> &DeviceName {
        &self.name
    }

    pub const fn measures(&self) -> &BTreeSet<Measurand> {
        &self.measures
    }

    pub const fn first_used(&self) -> Date {
        self.first_used
    }

    pub const fn last_used(&self) -> LastUsed {
        self.last_used
    }
}

/// Something the operator says about a device the register holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceStatement {
    FirstUsed {
        device: DeviceIdentifier,
        on: Date,
    },
    LastUsed {
        device: DeviceIdentifier,
        last: LastUsed,
    },
    Named {
        device: DeviceIdentifier,
        name: DeviceName,
    },
    /// What it sends to another device, which replaces what was said or
    /// recorded before.
    Sends {
        device: DeviceIdentifier,
        what: BTreeSet<Measurand>,
    },
    /// The two identifiers are one device.
    SameDevice {
        device: DeviceIdentifier,
        other: DeviceIdentifier,
    },
}

/// Everything the operator says about his devices.
///
/// **Adding one is not a statement about one**, because there is nothing yet
/// to address: the store numbers the device as it keeps the statement, and
/// that number is what later statements are addressed to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Added {
        device: StatedDevice,
        added: AddedDevice,
    },
    About(DeviceStatement),
}

/// A statement, and when it was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stated {
    pub at: Timestamp,
    pub statement: Statement,
}

/// Where a value in the register came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// The first or last recording the source holds on the device.
    Recorded { source: &'static str },
    /// The operator said so.
    Stated { at: Timestamp },
}

/// A value, and where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sourced<T> {
    pub value: T,
    pub basis: Basis,
}

/// One real-world device, and when it was in use.
///
/// **The period is ordered by construction**: a statement that would put the
/// last use before the first is not applied (see [`register`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    identifiers: Vec<DeviceIdentifier>,
    name: Sourced<DeviceName>,
    measures: BTreeSet<Measurand>,
    sends: Sourced<BTreeSet<Measurand>>,
    first_used: Sourced<Date>,
    last_used: Sourced<LastUsed>,
}

impl Device {
    /// Every identifier it reports or was given, the one a page should use
    /// first.
    pub fn identifiers(&self) -> &[DeviceIdentifier] {
        &self.identifiers
    }

    /// The identifier anything said about it is addressed to.
    pub fn identifier(&self) -> Option<&DeviceIdentifier> {
        self.identifiers.first()
    }

    pub fn is(&self, identifier: &DeviceIdentifier) -> bool {
        self.identifiers.contains(identifier)
    }

    pub const fn name(&self) -> &Sourced<DeviceName> {
        &self.name
    }

    pub const fn measures(&self) -> &BTreeSet<Measurand> {
        &self.measures
    }

    /// What it sends to another device, which is what makes it a candidate
    /// for a reading that arrives without naming its device: a strap's heart
    /// rate the Bike+ relays. A scale measures a pulse and sends it nowhere.
    pub const fn sends(&self) -> &Sourced<BTreeSet<Measurand>> {
        &self.sends
    }

    pub const fn first_used(&self) -> &Sourced<Date> {
        &self.first_used
    }

    pub const fn last_used(&self) -> &Sourced<LastUsed> {
        &self.last_used
    }

    /// Whether it was in use on `date`.
    #[must_use]
    pub fn in_use_on(&self, date: Date) -> bool {
        self.first_used.value <= date && self.last_used.value.covers(date)
    }
}

/// Why a statement would leave the register wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Contradiction {
    #[error("no device is identified as {0}")]
    UnknownDevice(DeviceIdentifier),
    #[error("a device cannot be last used ({last}) before it was first used ({first})")]
    LastBeforeFirst { first: Date, last: LastUsed },
    #[error("{0} is already that device")]
    AlreadyOne(DeviceIdentifier),
}

/// The register: what the record shows, with what the operator stated applied.
///
/// Deterministic (§ 9), and ordered by first use. Statements apply in the
/// order they were made, so the latest of each kind stands; one naming no
/// device the register holds, or putting a device's last use before its first,
/// changes nothing.
#[must_use]
pub fn register(sightings: &[Sighting], statements: &[Stated]) -> Vec<Device> {
    let mut devices = recorded(sightings);
    let mut ordered: Vec<&Stated> = statements.iter().collect();
    ordered.sort_by_key(|stated| stated.at);
    for stated in ordered {
        let _ = apply(&mut devices, stated);
    }
    devices.sort_by(|one, other| {
        (one.first_used.value, &one.identifiers).cmp(&(other.first_used.value, &other.identifiers))
    });
    devices
}

/// Whether a statement can be made against the register as it stands.
///
/// # Errors
///
/// [`Contradiction`] naming why it would not be applied.
pub fn check(devices: &[Device], stated: &Stated) -> Result<(), Contradiction> {
    let mut devices = devices.to_vec();
    apply(&mut devices, stated)
}

/// The device of what the record shows, one per identifier.
fn recorded(sightings: &[Sighting]) -> Vec<Device> {
    let mut devices: Vec<(Device, bool)> = Vec::new();
    for sighting in sightings {
        let date = sighting.at.wall_clock().date();
        let recorded = Basis::Recorded {
            source: sighting.source,
        };
        let wrote_it = sighting.role == Role::Recorder;
        let identifiers: Vec<DeviceIdentifier> = std::iter::once(&sighting.device)
            .chain(&sighting.also)
            .cloned()
            .collect();
        let mut sighted = (
            Device {
                identifiers: identifiers.clone(),
                name: Sourced {
                    value: sighting.name.clone(),
                    basis: recorded,
                },
                measures: sighting.measured.clone(),
                sends: Sourced {
                    value: sighting.sent.clone(),
                    basis: recorded,
                },
                first_used: Sourced {
                    value: date,
                    basis: recorded,
                },
                last_used: Sourced {
                    value: LastUsed::On(date),
                    basis: recorded,
                },
            },
            wrote_it,
        );
        // Every device this sighting names by any identifier is the same one:
        // a serial's ANT+ number joins a watch to the monitor the Bike+ is
        // paired with.
        let mut index = 0;
        while index < devices.len() {
            let shares = devices
                .get(index)
                .is_some_and(|(held, _)| identifiers.iter().any(|identifier| held.is(identifier)));
            if shares {
                let (held, named_as_recorder) = devices.remove(index);
                sighted = merged((held, named_as_recorder), sighted);
            } else {
                index = index.saturating_add(1);
            }
        }
        devices.push(sighted);
    }
    devices.into_iter().map(|(device, _)| device).collect()
}

/// Two accounts of one device from the record, the one already held first.
///
/// **What it calls itself when it wrote a recording is its name**, rather than
/// what something it was paired to called it.
fn merged(held: (Device, bool), sighted: (Device, bool)) -> (Device, bool) {
    let (mut device, held_as_recorder) = held;
    let (other, sighted_as_recorder) = sighted;
    let name = if sighted_as_recorder && !held_as_recorder {
        other.name.clone()
    } else {
        device.name.clone()
    };
    for identifier in &other.identifiers {
        if !device.is(identifier) {
            device.identifiers.push(identifier.clone());
        }
    }
    device.measures.extend(other.measures.iter().copied());
    if !other.sends.value.is_subset(&device.sends.value) {
        device.sends.value.extend(other.sends.value.iter().copied());
        device.sends.basis = other.sends.basis;
    }
    if other.first_used.value < device.first_used.value {
        device.first_used = other.first_used;
    }
    if let (LastUsed::On(mine), LastUsed::On(theirs)) =
        (device.last_used.value, other.last_used.value)
        && theirs > mine
    {
        device.last_used = other.last_used;
    }
    device.name = name;
    (device, held_as_recorder || sighted_as_recorder)
}

fn apply(devices: &mut Vec<Device>, stated: &Stated) -> Result<(), Contradiction> {
    let basis = Basis::Stated { at: stated.at };
    let find = |devices: &[Device], identifier: &DeviceIdentifier| {
        devices
            .iter()
            .position(|device| device.is(identifier))
            .ok_or_else(|| Contradiction::UnknownDevice(identifier.clone()))
    };
    match &stated.statement {
        Statement::Added { device, added } => {
            devices.push(Device {
                identifiers: vec![DeviceIdentifier::Stated(*device)],
                name: Sourced {
                    value: added.name.clone(),
                    basis,
                },
                measures: added.measures.clone(),
                sends: Sourced {
                    value: BTreeSet::new(),
                    basis,
                },
                first_used: Sourced {
                    value: added.first_used,
                    basis,
                },
                last_used: Sourced {
                    value: added.last_used,
                    basis,
                },
            });
        }
        Statement::About(DeviceStatement::FirstUsed { device, on }) => {
            let index = find(devices, device)?;
            let held = devices
                .get_mut(index)
                .ok_or_else(|| Contradiction::UnknownDevice(device.clone()))?;
            if !held.last_used.value.covers(*on) {
                return Err(Contradiction::LastBeforeFirst {
                    first: *on,
                    last: held.last_used.value,
                });
            }
            held.first_used = Sourced { value: *on, basis };
        }
        Statement::About(DeviceStatement::LastUsed { device, last }) => {
            let index = find(devices, device)?;
            let held = devices
                .get_mut(index)
                .ok_or_else(|| Contradiction::UnknownDevice(device.clone()))?;
            if !last.covers(held.first_used.value) {
                return Err(Contradiction::LastBeforeFirst {
                    first: held.first_used.value,
                    last: *last,
                });
            }
            held.last_used = Sourced {
                value: *last,
                basis,
            };
        }
        Statement::About(DeviceStatement::Named { device, name }) => {
            let index = find(devices, device)?;
            if let Some(held) = devices.get_mut(index) {
                held.name = Sourced {
                    value: name.clone(),
                    basis,
                };
            }
        }
        Statement::About(DeviceStatement::Sends { device, what }) => {
            let index = find(devices, device)?;
            if let Some(held) = devices.get_mut(index) {
                held.measures.extend(what.iter().copied());
                held.sends = Sourced {
                    value: what.clone(),
                    basis,
                };
            }
        }
        Statement::About(DeviceStatement::SameDevice { device, other }) => {
            let kept = find(devices, device)?;
            let merged = find(devices, other)?;
            if kept == merged {
                return Err(Contradiction::AlreadyOne(other.clone()));
            }
            let absorbed = devices.remove(merged);
            let kept = if merged < kept {
                kept.saturating_sub(1)
            } else {
                kept
            };
            if let Some(held) = devices.get_mut(kept) {
                held.absorb(absorbed);
            }
        }
    }
    Ok(())
}

impl Device {
    /// Take in another account of this device: every identifier, everything
    /// either measured, and the widest period the two give.
    fn absorb(&mut self, other: Self) {
        for identifier in other.identifiers {
            if !self.is(&identifier) {
                self.identifiers.push(identifier);
            }
        }
        self.measures.extend(other.measures);
        self.sends.value.extend(other.sends.value);
        if other.first_used.value < self.first_used.value {
            self.first_used = other.first_used;
        }
        let later = match (self.last_used.value, other.last_used.value) {
            (LastUsed::StillInUse, _) => false,
            (LastUsed::On(_), LastUsed::StillInUse) => true,
            (LastUsed::On(mine), LastUsed::On(theirs)) => theirs > mine,
        };
        if later {
            self.last_used = other.last_used;
        }
    }
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
/// could have sent it, and a scale, which measures a pulse for itself, could
/// not (operator, 2026-10-13).
///
/// **One, or it is not known.** Two candidates in use is two candidates, and
/// choosing between them would be a guess (§ 37).
#[must_use]
pub fn sent_by(devices: &[Device], what: Measurand, date: Date) -> InUse<'_> {
    let candidates: Vec<&Device> = devices
        .iter()
        .filter(|device| device.sends.value.contains(&what) && device.in_use_on(date))
        .collect();
    match candidates.as_slice() {
        [] => InUse::Unknown,
        [one] => InUse::One(one),
        _ => InUse::Several(candidates),
    }
}
