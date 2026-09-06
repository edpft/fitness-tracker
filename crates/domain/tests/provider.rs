//! What a published programme is called, and which of its weeks we took.

use domain::provider::{
    ExternalProgramme, InvalidProgrammeName, InvalidProvider, InvalidProvision, ProgrammeName,
    ProvidedFrom, Provider,
};

fn name(text: &str) -> Result<ProgrammeName, InvalidProgrammeName> {
    ProgrammeName::try_from(text.to_owned())
}

fn provider(text: &str) -> Result<Provider, InvalidProvider> {
    Provider::try_from(text.to_owned())
}

fn peloton(
    programme: &str,
    microcycles: Vec<u32>,
) -> Result<ProvidedFrom, Box<dyn std::error::Error>> {
    Ok(ProvidedFrom::new(
        ExternalProgramme::new(provider("Peloton")?, name(programme)?),
        microcycles,
    )?)
}

/// Surrounding whitespace is trimmed rather than refused.
///
/// **Because it is compared.** Two transcriptions of one Peloton title must not
/// become two programmes, and one of them will sooner or later carry a space.
#[test]
fn a_name_is_trimmed_so_one_title_is_one_programme() {
    let (Ok(bare), Ok(padded)) = (
        name("Peak Your Power Zones"),
        name(" Peak Your Power Zones "),
    ) else {
        panic!("both are usable names")
    };
    assert_eq!(bare, padded);
}

#[test]
fn a_name_must_be_one_printable_line() {
    assert_eq!(name(""), Err(InvalidProgrammeName::Empty));
    assert_eq!(name("   "), Err(InvalidProgrammeName::Empty));
    assert_eq!(
        name("Peak Your\nPower Zones"),
        Err(InvalidProgrammeName::NotPrintable)
    );
    let long = "a".repeat(65);
    assert_eq!(
        name(&long),
        Err(InvalidProgrammeName::TooLong { length: 65 })
    );
}

/// The provider is part of the identity, so one title from two publishers is
/// two programmes.
#[test]
fn a_published_programme_is_its_publisher_and_its_title() {
    let (Ok(peloton), Ok(sbs), Ok(title)) = (
        provider("Peloton"),
        provider("Stronger By Science"),
        name("Build Your Power Zones"),
    ) else {
        panic!("all three are usable names")
    };
    assert_ne!(
        ExternalProgramme::new(peloton, title.clone()),
        ExternalProgramme::new(sbs, title)
    );
}

/// A selection keeps the published numbering, not ours.
#[test]
fn a_selection_prints_the_weeks_it_took() {
    let Ok(build) = peloton("Build Your Power Zones", vec![1, 2, 4, 5]) else {
        panic!("the selection is valid")
    };
    assert_eq!(build.to_string(), "micros 1-2-4-5");
    assert_eq!(build.microcycles().collect::<Vec<_>>(), vec![1, 2, 4, 5]);
}

/// One week is `micro 5`, not `micros 5`. The entry test is that case.
#[test]
fn one_week_is_singular() {
    let Ok(entry) = peloton("Build Your Power Zones", vec![5]) else {
        panic!("the selection is valid")
    };
    assert_eq!(entry.to_string(), "micro 5");
}

/// µ1-1-2 is not a selection anybody made.
#[test]
fn a_week_is_taken_once() {
    let Ok(programme) = (|| -> Result<ExternalProgramme, Box<dyn std::error::Error>> {
        Ok(ExternalProgramme::new(
            provider("Peloton")?,
            name("Build Your Power Zones")?,
        ))
    })() else {
        panic!("the programme is valid")
    };
    assert_eq!(
        ProvidedFrom::new(programme.clone(), vec![1, 1, 2]),
        Err(InvalidProvision::RepeatedMicrocycle { microcycle: 1 })
    );
    assert_eq!(
        ProvidedFrom::new(programme.clone(), vec![0]),
        Err(InvalidProvision::ZeroMicrocycle)
    );
    assert_eq!(
        ProvidedFrom::new(programme, Vec::new()),
        Err(InvalidProvision::NoMicrocycles)
    );
}
