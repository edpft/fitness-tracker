//! The address of a manual weigh-in, and what one may hold (#273).

use domain::{
    body::{Cell, CellRef, InvalidWeighIn, ManualWeighIn, SheetName},
    landing::{FilePath, LandingRecordId, SourceRecordId},
    measure::Kg,
};
use jiff::civil::date;

#[test]
fn a_cell_is_addressed_as_a_spreadsheet_shows_it() {
    assert_eq!(CellRef::at(0, 0).as_str(), "A1");
    assert_eq!(CellRef::at(5, 2).as_str(), "C6");
    assert_eq!(CellRef::at(1463, 1).as_str(), "B1464");
    assert_eq!(CellRef::at(0, 25).as_str(), "Z1");
    assert_eq!(CellRef::at(0, 26).as_str(), "AA1");
    assert_eq!(CellRef::at(0, 701).as_str(), "ZZ1");
    assert_eq!(CellRef::at(0, 702).as_str(), "AAA1");
}

#[test]
fn only_a_cell_reference_is_a_cell_reference() {
    for reference in ["C6", "AA10", "XFD1048576"] {
        assert!(CellRef::try_from(reference).is_ok(), "{reference}");
    }
    for reference in ["", "6", "C", "C0", "c6", "C06", "ABCD1", "C6:D7"] {
        assert!(CellRef::try_from(reference).is_err(), "{reference}");
    }
}

#[test]
fn a_weigh_in_of_no_mass_is_not_a_weigh_in() {
    let cell = Cell {
        landed_as: LandingRecordId::try_from(1).expect("an id"),
        source_record_id: SourceRecordId::try_from("digest").expect("an id"),
        file: FilePath::try_from("Dropbox/Random/Body Weight.xlsx").expect("a path"),
        sheet: SheetName::try_from("2016").expect("a sheet"),
        cell: CellRef::at(5, 2),
    };

    assert_eq!(
        ManualWeighIn::new(date(2016, 1, 5), Kg::NONE, cell.clone()),
        Err(InvalidWeighIn::NoMass)
    );
    let weigh_in =
        ManualWeighIn::new(date(2016, 1, 5), Kg::from_grams(66_300), cell).expect("a weigh-in");
    assert_eq!(weigh_in.mass().to_string(), "66.3");
}
