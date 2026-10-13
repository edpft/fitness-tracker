//! Reading a heart rate out of the recording a watch wrote.
//!
//! **The bytes landed as served (§ II.1), so this is where they are opened.**
//! `/download-service/files/activity/{id}` answers with a zip holding one FIT
//! file, and [`super::files`] lands that archive without decompressing anything.
//! What the activity list states about the session's heart rate is an average
//! and a highest; the readings they summarise are here. The operator,
//! 2026-09-18: *"heart rate for the canonical session needs per-second samples,
//! not `averageHR`/`maxHR`"*.
//!
//! **Two fields of one message, not a decoder for the format.** A FIT file
//! describes each message's fields before serving them, so skipping what we do
//! not read needs only the declared sizes — no table of Garmin's field numbers,
//! no profile version, and nothing that has to be revised when the watch starts
//! writing a field this build has never seen. `fitparser` would hand back a
//! `Value` per field of all 8,700 messages in one of the operator's sessions,
//! and reach `chrono` and `iana-time-zone` to do it, which § 34 rules out for
//! wall clocks anyway.
//!
//! **What a reading is placed against is the file's own clock.** FIT counts
//! seconds from 1989-12-31, and on 16 of the operator's 2015 and 2016 sessions
//! the watch had no time fix and wrote 2007-04-01 — so an absolute instant from
//! here would contradict the session's start by nine years. The offsets are
//! sound on every one of them, which is what a series needs, and the session's
//! own clock stays the activity list's.

use domain::{
    measure::{BeatsPerMinute, Duration, HeartRateSample},
    sequence::NonEmpty,
};

/// Why a recording could not be read.
///
/// Every variant is the file saying something this cannot follow, which costs
/// the series and not the session: the summary the activity list states stands
/// on its own.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UnreadableRecording {
    #[error("the archive will not open: {detail}")]
    Archive { detail: String },
    #[error("an archive holding {entries} files is not one recording")]
    NotOneFile { entries: usize },
    #[error("{detail}")]
    NotFit { detail: &'static str },
    #[error("a message at byte {at} claims fields the file does not hold")]
    Truncated { at: usize },
    #[error("a message at byte {at} was served before its definition")]
    Undefined { at: usize },
}

/// The readings the recording holds, in the order the watch wrote them.
///
/// [`None`] where the file reads but holds no reading at all: 20 of the
/// operator's 551 strength activities, every one of which states no summary
/// either. A zero is not a reading — see [`BeatsPerMinute`] — so a strap that
/// dropped out for the whole session is this rather than a series of zeros.
///
/// # Errors
///
/// [`UnreadableRecording`] if the archive or the file inside it will not read.
pub fn heart_rate(
    archive: &[u8],
) -> Result<Option<NonEmpty<HeartRateSample>>, UnreadableRecording> {
    let recording = only_file(archive)?;
    let readings = readings(&recording)?;
    Ok(NonEmpty::new(readings).ok())
}

/// The one file the archive holds.
///
/// **One, or none of them**, because a second file would make "the recording"
/// a choice this cannot make. Every one of the operator's 2,276 landed archives
/// holds exactly one entry, named for its activity.
fn only_file(archive: &[u8]) -> Result<Vec<u8>, UnreadableRecording> {
    use std::io::Read as _;

    let mut opened = zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(|error| {
        UnreadableRecording::Archive {
            detail: error.to_string(),
        }
    })?;
    if opened.len() != 1 {
        return Err(UnreadableRecording::NotOneFile {
            entries: opened.len(),
        });
    }
    let mut entry = opened
        .by_index(0)
        .map_err(|error| UnreadableRecording::Archive {
            detail: error.to_string(),
        })?;
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or_default());
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| UnreadableRecording::Archive {
            detail: error.to_string(),
        })?;
    Ok(bytes)
}

/// The global message number of an activity's `record`, which carries a reading
/// per moment, and of the `session` that states when the recording began.
const RECORD: u16 = 20;
const SESSION: u16 = 18;

/// `timestamp`, which every timed message carries under the same number, and
/// `heart_rate` on a record. The FIT profile fixes both.
const TIMESTAMP: u8 = 253;
const HEART_RATE: u8 = 3;
/// `start_time` on a session.
const START_TIME: u8 = 2;

/// What a `uint32` and a `uint8` are when the field was not measured.
const NO_UINT32: u32 = 0xFFFF_FFFF;
const NO_UINT8: u8 = 0xFF;

/// One field of a message, as its definition declares it.
///
/// The base type is not kept: what this needs from a field it does not read is
/// how many bytes to step over, and that is the size.
#[derive(Debug, Clone, Copy)]
struct Field {
    number: u8,
    size: usize,
}

/// A local message type, as the definition that claimed it left it.
#[derive(Debug, Clone)]
struct Definition {
    global: u16,
    fields: Vec<Field>,
    little_endian: bool,
}

/// A reading and the moment the file put it at, before either has a base to
/// count from.
struct Stamped {
    at: u32,
    beats_per_minute: BeatsPerMinute,
}

/// Every reading in the file, placed against the moment the recording began.
fn readings(file: &[u8]) -> Result<Vec<HeartRateSample>, UnreadableRecording> {
    let (mut at, end) = extent(file)?;
    let mut definitions: [Option<Definition>; 16] = std::array::from_fn(|_| None);
    let mut stated_start = None;
    let mut first_record = None;
    let mut stamped: Vec<Stamped> = Vec::new();
    let mut latest = None;

    while at < end {
        let header = *file.get(at).ok_or(UnreadableRecording::Truncated { at })?;
        let started_at = at;
        at = at.saturating_add(1);

        if header & 0x80 != 0 {
            // A compressed timestamp header: the local type is two bits, and
            // the remaining five carry the low bits of a timestamp counted from
            // the last one served in full. No file of the operator's uses one,
            // so this is here for the format rather than for his record.
            let local = usize::from((header >> 5) & 0x03);
            let definition = definitions
                .get(local)
                .and_then(Option::as_ref)
                .ok_or(UnreadableRecording::Undefined { at: started_at })?;
            let values = read_message(file, &mut at, definition, started_at)?;
            let stamp = latest.map(|previous| rolled_over(previous, header & 0x1F));
            if let Some(stamp) = stamp {
                latest = Some(stamp);
                if definition.global == RECORD {
                    first_record = first_record.or(Some(stamp));
                    if let Some(beats_per_minute) = beats(values.heart_rate) {
                        stamped.push(Stamped {
                            at: stamp,
                            beats_per_minute,
                        });
                    }
                }
            }
            continue;
        }

        let local = usize::from(header & 0x0F);
        if header & 0x40 != 0 {
            let definition = read_definition(file, &mut at, header, started_at)?;
            if let Some(slot) = definitions.get_mut(local) {
                *slot = Some(definition);
            }
            continue;
        }

        let definition = definitions
            .get(local)
            .and_then(Option::as_ref)
            .ok_or(UnreadableRecording::Undefined { at: started_at })?;
        let global = definition.global;
        let values = read_message(file, &mut at, definition, started_at)?;
        if let Some(stamp) = values.timestamp {
            latest = Some(stamp);
        }
        match global {
            SESSION => stated_start = stated_start.or(values.start_time),
            RECORD => {
                if let Some(stamp) = values.timestamp {
                    first_record = first_record.or(Some(stamp));
                    if let Some(beats_per_minute) = beats(values.heart_rate) {
                        stamped.push(Stamped {
                            at: stamp,
                            beats_per_minute,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    Ok(placed(&stamped, base(stated_start, first_record)))
}

/// The earliest moment the file itself puts the recording at.
///
/// **The earlier of the two statements it makes**, so no reading can precede the
/// start it is counted from. The session states one and the first record carries
/// one: on 522 of the operator's 531 recordings they are the same second, and on
/// the other nine the session's is a second earlier.
const fn base(stated_start: Option<u32>, first_record: Option<u32>) -> Option<u32> {
    match (stated_start, first_record) {
        (Some(stated), Some(first)) if first < stated => Some(first),
        (Some(stated), _) => Some(stated),
        (None, first) => first,
    }
}

/// Each reading at its offset from the start of the recording.
fn placed(stamped: &[Stamped], base: Option<u32>) -> Vec<HeartRateSample> {
    let Some(base) = base else {
        return Vec::new();
    };
    stamped
        .iter()
        .map(|reading| HeartRateSample {
            at: Duration::from_seconds(u64::from(reading.at.saturating_sub(base))),
            beats_per_minute: reading.beats_per_minute,
        })
        .collect()
}

/// A reading, where the file held one. A `0xFF` is the field saying nothing, and
/// a zero is a strap saying nothing — [`BeatsPerMinute`] refuses that one.
fn beats(stated: Option<u8>) -> Option<BeatsPerMinute> {
    let stated = stated.filter(|beats| *beats != NO_UINT8)?;
    BeatsPerMinute::new(u32::from(stated)).ok()
}

/// A full timestamp from the five bits a compressed header carries.
///
/// The offset replaces the low five bits of the last full timestamp, and rolls
/// the sixth when it runs backwards past it (FIT protocol 4.2.2).
fn rolled_over(previous: u32, offset: u8) -> u32 {
    let offset = u32::from(offset);
    previous.wrapping_add(offset.wrapping_sub(previous) & 0x1F)
}

/// Where the records start and where they end.
///
/// The header states its own length and how many bytes of records follow; a CRC
/// sits after them, and a second FIT file may follow that. Only the first is
/// read: Garmin serves one recording per activity.
fn extent(file: &[u8]) -> Result<(usize, usize), UnreadableRecording> {
    let header = usize::from(*file.first().ok_or(UnreadableRecording::NotFit {
        detail: "the file is empty",
    })?);
    if header < 12 || file.get(8..12) != Some(b".FIT") {
        return Err(UnreadableRecording::NotFit {
            detail: "the file does not declare itself a FIT recording",
        });
    }
    let stated = u32_at(file, 4, true).ok_or(UnreadableRecording::NotFit {
        detail: "the header states no length",
    })?;
    let end = header
        .checked_add(usize::try_from(stated).unwrap_or(usize::MAX))
        .ok_or(UnreadableRecording::NotFit {
            detail: "the header states a length past the end of the file",
        })?;
    Ok((header, end.min(file.len())))
}

/// The definition a header at `at` introduces, stepping `at` past it.
fn read_definition(
    file: &[u8],
    at: &mut usize,
    header: u8,
    started_at: usize,
) -> Result<Definition, UnreadableRecording> {
    let truncated = || UnreadableRecording::Truncated { at: started_at };
    // One reserved byte, then the architecture every multi-byte field in this
    // message is written in.
    let architecture = *file.get(at.saturating_add(1)).ok_or_else(truncated)?;
    let little_endian = architecture == 0;
    let global = u16_at(file, at.saturating_add(2), little_endian).ok_or_else(truncated)?;
    let count = usize::from(*file.get(at.saturating_add(4)).ok_or_else(truncated)?);

    let mut fields = Vec::with_capacity(count);
    let mut reading = at.saturating_add(5);
    for _ in 0..count {
        let declared = declaration(file, reading).ok_or_else(truncated)?;
        fields.push(declared);
        reading = reading.saturating_add(3);
    }

    // A developer field carries no profile number, so it is never one this
    // reads — but its bytes are part of the message, so its size is declared
    // here and stepped over with the rest.
    if header & 0x20 != 0 {
        let developer = usize::from(*file.get(reading).ok_or_else(truncated)?);
        reading = reading.saturating_add(1);
        for _ in 0..developer {
            let declared = declaration(file, reading).ok_or_else(truncated)?;
            fields.push(Field {
                number: NO_UINT8,
                size: declared.size,
            });
            reading = reading.saturating_add(3);
        }
    }

    *at = reading;
    Ok(Definition {
        global,
        fields,
        little_endian,
    })
}

/// One field's number and size, out of the three bytes that declare it.
fn declaration(file: &[u8], at: usize) -> Option<Field> {
    let declared = file.get(at..at.saturating_add(3))?;
    Some(Field {
        number: *declared.first()?,
        size: usize::from(*declared.get(1)?),
    })
}

/// The three fields this reads, out of however many a message serves.
#[derive(Debug, Default)]
struct Values {
    timestamp: Option<u32>,
    start_time: Option<u32>,
    heart_rate: Option<u8>,
}

/// One data message, stepping `at` past every field its definition declares.
fn read_message(
    file: &[u8],
    at: &mut usize,
    definition: &Definition,
    started_at: usize,
) -> Result<Values, UnreadableRecording> {
    let mut values = Values::default();
    for field in &definition.fields {
        let ends_at = at
            .checked_add(field.size)
            .ok_or(UnreadableRecording::Truncated { at: started_at })?;
        if ends_at > file.len() {
            return Err(UnreadableRecording::Truncated { at: started_at });
        }
        match (field.number, field.size) {
            (TIMESTAMP, 4) => values.timestamp = whole(file, *at, definition.little_endian),
            (START_TIME, 4) if definition.global == SESSION => {
                values.start_time = whole(file, *at, definition.little_endian);
            }
            (HEART_RATE, 1) if definition.global == RECORD => {
                values.heart_rate = file.get(*at).copied();
            }
            _ => {}
        }
        *at = ends_at;
    }
    Ok(values)
}

/// A `uint32` the file measured, or [`None`] where it says it did not.
fn whole(file: &[u8], at: usize, little_endian: bool) -> Option<u32> {
    u32_at(file, at, little_endian).filter(|stated| *stated != NO_UINT32)
}

/// Four bytes as the architecture the definition declared.
fn u32_at(file: &[u8], at: usize, little_endian: bool) -> Option<u32> {
    let bytes: [u8; 4] = file.get(at..at.saturating_add(4))?.try_into().ok()?;
    Some(if little_endian {
        u32::from_le_bytes(bytes)
    } else {
        u32::from_be_bytes(bytes)
    })
}

/// Two bytes, the same way.
fn u16_at(file: &[u8], at: usize, little_endian: bool) -> Option<u16> {
    let bytes: [u8; 2] = file.get(at..at.saturating_add(2))?.try_into().ok()?;
    Some(if little_endian {
        u16::from_le_bytes(bytes)
    } else {
        u16::from_be_bytes(bytes)
    })
}

/// What a recording says about the hardware that made it.
///
/// **The file's own account, which the activity list does not give.** The list
/// names one device id per activity, and a peloton-to-garmin upload presents as
/// a watch the operator never owned. The file names the device that wrote it
/// (`file_id`) and every sensor paired to it (`device_info`), each by the serial
/// number the hardware reports — which is what tells two straps of one model
/// apart (#427).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hardware {
    /// What wrote the recording, as `file_id` states it.
    pub recorder: Identity,
    /// Everything `device_info` lists, the recorder's own entry among them.
    pub listed: Vec<Listed>,
}

/// Who made a device, which of their products it is, and its serial number.
///
/// Each is [`None`] where the file says it does not know, which is what FIT's
/// invalid values mean.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Identity {
    pub manufacturer: Option<u16>,
    pub product: Option<u16>,
    pub serial: Option<u32>,
}

/// One `device_info` entry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Listed {
    /// Which device of the recording this is; zero is the recorder itself.
    pub index: Option<u8>,
    pub identity: Identity,
    /// The device type, read against [`Self::link`]'s table.
    pub kind: Option<u8>,
    /// How it was connected: ANT, ANT+, Bluetooth, onboard. Old files leave it
    /// out.
    pub link: Option<u8>,
}

/// The hardware a recording names.
///
/// # Errors
///
/// [`UnreadableRecording`] if the archive or the file inside it will not read.
pub fn hardware(archive: &[u8]) -> Result<Hardware, UnreadableRecording> {
    let recording = only_file(archive)?;
    identities(&recording)
}

/// `file_id` and `device_info`, and the fields of each that identify hardware.
/// The FIT profile fixes every number.
const FILE_ID: u16 = 0;
const DEVICE_INFO: u16 = 23;
const FILE_ID_MANUFACTURER: u8 = 1;
const FILE_ID_PRODUCT: u8 = 2;
const FILE_ID_SERIAL: u8 = 3;
const DEVICE_INDEX: u8 = 0;
const DEVICE_TYPE: u8 = 1;
const DEVICE_MANUFACTURER: u8 = 2;
const DEVICE_SERIAL: u8 = 3;
const DEVICE_PRODUCT: u8 = 4;
const SOURCE_TYPE: u8 = 25;

/// What a `uint16` is when the field was not measured.
const NO_UINT16: u16 = 0xFFFF;

fn identities(file: &[u8]) -> Result<Hardware, UnreadableRecording> {
    let (mut at, end) = extent(file)?;
    let mut definitions: [Option<Definition>; 16] = std::array::from_fn(|_| None);
    let mut hardware = Hardware::default();
    let mut recorder_stated = false;

    while at < end {
        let header = *file.get(at).ok_or(UnreadableRecording::Truncated { at })?;
        let started_at = at;
        at = at.saturating_add(1);

        let local = if header & 0x80 == 0 {
            if header & 0x40 != 0 {
                let definition = read_definition(file, &mut at, header, started_at)?;
                if let Some(slot) = definitions.get_mut(usize::from(header & 0x0F)) {
                    *slot = Some(definition);
                }
                continue;
            }
            usize::from(header & 0x0F)
        } else {
            usize::from((header >> 5) & 0x03)
        };
        let definition = definitions
            .get(local)
            .and_then(Option::as_ref)
            .ok_or(UnreadableRecording::Undefined { at: started_at })?;
        let fields = numbers(file, &mut at, definition, started_at)?;
        let field = |number: u8| {
            fields
                .iter()
                .find(|(declared, _)| *declared == number)
                .and_then(|(_, value)| *value)
        };
        let byte = |number: u8| {
            field(number)
                .and_then(|value| u8::try_from(value).ok())
                .filter(|value| *value != NO_UINT8)
        };
        let short = |number: u8| {
            field(number)
                .and_then(|value| u16::try_from(value).ok())
                .filter(|value| *value != NO_UINT16)
        };
        let serial = |number: u8| {
            field(number)
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value != 0 && *value != NO_UINT32)
        };
        match definition.global {
            FILE_ID if !recorder_stated => {
                recorder_stated = true;
                hardware.recorder = Identity {
                    manufacturer: short(FILE_ID_MANUFACTURER),
                    product: short(FILE_ID_PRODUCT),
                    serial: serial(FILE_ID_SERIAL),
                };
            }
            DEVICE_INFO => hardware.listed.push(Listed {
                index: byte(DEVICE_INDEX),
                identity: Identity {
                    manufacturer: short(DEVICE_MANUFACTURER),
                    product: short(DEVICE_PRODUCT),
                    serial: serial(DEVICE_SERIAL),
                },
                kind: byte(DEVICE_TYPE),
                link: byte(SOURCE_TYPE),
            }),
            _ => {}
        }
    }

    Ok(hardware)
}

/// Every one-, two- and four-byte field of a message as an unsigned number,
/// stepping `at` past the whole message. Anything else is stepped over.
fn numbers(
    file: &[u8],
    at: &mut usize,
    definition: &Definition,
    started_at: usize,
) -> Result<Vec<(u8, Option<u64>)>, UnreadableRecording> {
    let mut fields = Vec::with_capacity(definition.fields.len());
    for field in &definition.fields {
        let ends_at = at
            .checked_add(field.size)
            .ok_or(UnreadableRecording::Truncated { at: started_at })?;
        if ends_at > file.len() {
            return Err(UnreadableRecording::Truncated { at: started_at });
        }
        let value = match field.size {
            1 => file.get(*at).copied().map(u64::from),
            2 => u16_at(file, *at, definition.little_endian).map(u64::from),
            4 => u32_at(file, *at, definition.little_endian).map(u64::from),
            _ => None,
        };
        fields.push((field.number, value));
        *at = ends_at;
    }
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::{Identity, Listed, UnreadableRecording, identities, readings};

    /// A FIT file around the messages a test writes.
    fn file(messages: &[u8]) -> Vec<u8> {
        let mut file = vec![12, 0x10, 0x00, 0x00];
        let length = u32::try_from(messages.len()).unwrap_or_default();
        file.extend_from_slice(&length.to_le_bytes());
        file.extend_from_slice(b".FIT");
        file.extend_from_slice(messages);
        file
    }

    /// The offsets and readings a file was read as.
    fn read(file: &[u8]) -> Result<Vec<(u64, u32)>, UnreadableRecording> {
        Ok(readings(file)?
            .into_iter()
            .map(|sample| (sample.at.as_seconds(), sample.beats_per_minute.as_u32()))
            .collect())
    }

    /// A `session` message stating when the recording began, little-endian.
    fn session(start: u32) -> Vec<u8> {
        let mut message = vec![0x40, 0x00, 0x00, 18, 0, 2, 253, 4, 0x86, 2, 4, 0x86, 0x00];
        message.extend_from_slice(&start.to_le_bytes());
        message.extend_from_slice(&start.to_le_bytes());
        message
    }

    #[test]
    fn a_compressed_header_carries_a_timestamp_counted_from_the_last_full_one() {
        // A watch may serve a record under a one-byte header whose low five bits
        // are the offset from the last timestamp served in full, rolling the
        // sixth bit when they run backwards past it (FIT protocol 4.2.2). None
        // of the operator's 2,276 files does, so nothing but this exercises it.
        let start = 920_000_000;
        let mut messages = session(start);
        // Local type 1 defines a record carrying a heart rate and no timestamp.
        messages.extend_from_slice(&[0x41, 0x00, 0x00, 20, 0, 1, 3, 1, 0x02]);
        for (offset, beats) in [(5_u8, 96_u8), (9, 98), (1, 133)] {
            messages.push(0x80 | 0x20 | offset);
            messages.push(beats);
        }

        assert_eq!(
            read(&file(&messages)).expect("a recording"),
            // The third rolls over: one is 24 seconds after nine, not eight
            // before it.
            vec![(5, 96), (9, 98), (33, 133)],
        );
    }

    #[test]
    fn a_definition_states_the_architecture_its_numbers_are_written_in() {
        let start = 920_000_000_u32;
        let mut messages = session(start);
        // The same record, declared big-endian.
        messages.extend_from_slice(&[0x41, 0x00, 0x01, 0, 20, 2, 253, 4, 0x86, 3, 1, 0x02]);
        messages.push(0x01);
        messages.extend_from_slice(&(start + 7).to_be_bytes());
        messages.push(101);

        assert_eq!(read(&file(&messages)).expect("a recording"), vec![(7, 101)]);
    }

    #[test]
    fn a_reading_the_field_says_it_did_not_take_is_not_a_reading() {
        let start = 920_000_000_u32;
        let mut messages = session(start);
        messages.extend_from_slice(&[0x41, 0x00, 0x00, 20, 0, 2, 253, 4, 0x86, 3, 1, 0x02]);
        for (at, beats) in [(start, 0xFF_u8), (start + 1, 0), (start + 2, 90)] {
            messages.push(0x01);
            messages.extend_from_slice(&at.to_le_bytes());
            messages.push(beats);
        }

        assert_eq!(read(&file(&messages)).expect("a recording"), vec![(2, 90)]);
    }

    #[test]
    fn a_message_without_a_definition_is_not_read_as_one() {
        assert_eq!(
            read(&file(&[0x00, 0x01, 0x02])),
            Err(UnreadableRecording::Undefined { at: 12 }),
        );
    }

    #[test]
    fn a_payload_that_is_not_a_recording_says_so() {
        assert!(matches!(
            read(b"not a FIT file at all"),
            Err(UnreadableRecording::NotFit { .. })
        ));
    }

    #[test]
    fn a_message_claiming_more_than_the_file_holds_is_truncated() {
        // The definition says four bytes of timestamp and the file holds one.
        let messages = [0x40, 0x00, 0x00, 20, 0, 1, 253, 4, 0x86, 0x00, 0x01];
        assert_eq!(
            read(&file(&messages)),
            Err(UnreadableRecording::Truncated { at: 21 }),
        );
    }

    #[test]
    fn a_recording_names_its_recorder_and_every_sensor_paired_to_it() {
        // `file_id`: manufacturer 1, product 4432, serial 3483897389.
        let mut messages = vec![
            0x40, 0x00, 0x00, 0, 0, 3, 1, 2, 0x84, 2, 2, 0x84, 3, 4, 0x8C,
        ];
        messages.push(0x00);
        messages.extend_from_slice(&1_u16.to_le_bytes());
        messages.extend_from_slice(&4432_u16.to_le_bytes());
        messages.extend_from_slice(&3_483_897_389_u32.to_le_bytes());
        // `device_info`: index, type, manufacturer, serial, product, source.
        messages.extend_from_slice(&[
            0x41, 0x00, 0x00, 23, 0, 6, 0, 1, 0x02, 1, 1, 0x02, 2, 2, 0x84, 3, 4, 0x8C, 4, 2, 0x84,
            25, 1, 0x00,
        ]);
        // A Wahoo strap over ANT+, then an entry whose serial says nothing.
        for (index, serial) in [(1_u8, 3_339_528_293_u32), (2, 0)] {
            messages.push(0x01);
            messages.extend_from_slice(&[index, 120]);
            messages.extend_from_slice(&32_u16.to_le_bytes());
            messages.extend_from_slice(&serial.to_le_bytes());
            messages.extend_from_slice(&0xFFFF_u16.to_le_bytes());
            messages.push(1);
        }

        let hardware = identities(&file(&messages)).expect("a recording");

        assert_eq!(
            hardware.recorder,
            Identity {
                manufacturer: Some(1),
                product: Some(4432),
                serial: Some(3_483_897_389),
            }
        );
        assert_eq!(
            hardware.listed,
            vec![
                Listed {
                    index: Some(1),
                    identity: Identity {
                        manufacturer: Some(32),
                        product: None,
                        serial: Some(3_339_528_293),
                    },
                    kind: Some(120),
                    link: Some(1),
                },
                Listed {
                    index: Some(2),
                    identity: Identity {
                        manufacturer: Some(32),
                        product: None,
                        serial: None,
                    },
                    kind: Some(120),
                    link: Some(1),
                },
            ]
        );
    }
}
