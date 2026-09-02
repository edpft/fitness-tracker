//! Which Peloton class realises each session of *Peak Your Power Zones*.
//!
//! Code, not data (§ 9), for the same reason `hevy::mapping` is: deterministic
//! translation must not be editable without review.
//!
//! **It lives here rather than in `domain` because it is keyed on `classId`,
//! which is a Peloton identifier.** What `domain` owns is the session this
//! points *at* — duration × power zone (decision 0025) — and the direction is
//! the whole of § 8. A domain holding a vendor's identifiers is a domain shaped
//! by a source.
//!
//! ## Titles inform this table and never key it
//!
//! Neither the title nor the instructor identifies a Peloton class, and the
//! programme demonstrates it three separate ways:
//!
//! - Week 6 day 1 and week 7 day 3 are **both** `45 min Power Zone Ride` by
//!   Christine D'Ercole.
//! - Two different `60 min Power Zone Endurance Ride` classes share the same
//!   13-minute warm-up and 46-minute ride.
//! - Two different `45 min Power Zone Endurance Ride` classes share a 10-minute
//!   warm-up and a 34-minute ride.
//!
//! The `classId` is the whole of what names a class. Titles are here to be read.
//!
//! ## What is deliberately absent
//!
//! **The `code` parameter of a share link.** A link the app produces carries
//! `…&code=<base64>&utm_source=…`, and that base64 decodes to two further
//! 32-hex identifiers, neither of which is the class. It is a share token and
//! one of the two is plausibly the operator's own Peloton user id, so it is not
//! committed — § 35 in spirit: a personal identifier is not repository content.
//!
//! **The date the class page shows.** Sixteen of the twenty-five read the
//! identical `Fri 13/5/22 @ 15:00` across five instructors, which no set of live
//! classes can share. It orders nothing and identifies nothing.

/// One Peloton class: what names it, and what it is called.
pub struct PelotonClass {
    /// The 32-hex identifier from the app's share link. The only unambiguous
    /// name a class has.
    class_id: &'static str,
    title: &'static str,
    instructor: &'static str,
    /// Whether the operator's account can currently start it.
    ///
    /// **One class of twenty-five cannot be**, and it is not transient: week 4
    /// day 6 read `Unavailable` across two captures an hour apart. Carried here
    /// rather than discovered at delivery so a prescription can say so.
    available: bool,
}

impl PelotonClass {
    pub const fn class_id(&self) -> &'static str {
        self.class_id
    }

    pub const fn title(&self) -> &'static str {
        self.title
    }

    pub const fn instructor(&self) -> &'static str {
        self.instructor
    }

    pub const fn available(&self) -> bool {
        self.available
    }

    /// Where the operator opens this class.
    ///
    /// Built from the `classId` alone — see the module note on what is stripped.
    #[must_use]
    pub fn url(&self) -> String {
        format!(
            "https://members.onepeloton.co.uk/classes/cycling?modal=classDetailsModal&classId={}",
            self.class_id
        )
    }
}

/// A session of the programme, and the class or classes that realise it.
pub struct MappedSession {
    week: u8,
    day: u8,
    /// **Usually one class, and for the FTP test two.** The app ships the test's
    /// warm-up as a separate class because the test itself has no warm-up
    /// section; what the operator rides is one session. See decision 0025.
    classes: &'static [PelotonClass],
}

impl MappedSession {
    pub const fn week(&self) -> u8 {
        self.week
    }

    pub const fn day(&self) -> u8 {
        self.day
    }

    pub const fn classes(&self) -> &'static [PelotonClass] {
        self.classes
    }

    /// Whether every class this session needs can be started.
    #[must_use]
    pub fn available(&self) -> bool {
        self.classes.iter().all(PelotonClass::available)
    }
}

const fn class(
    class_id: &'static str,
    title: &'static str,
    instructor: &'static str,
) -> PelotonClass {
    PelotonClass {
        class_id,
        title,
        instructor,
        available: true,
    }
}

const fn unavailable(
    class_id: &'static str,
    title: &'static str,
    instructor: &'static str,
) -> PelotonClass {
    PelotonClass {
        class_id,
        title,
        instructor,
        available: false,
    }
}

const ENDURANCE_45: &str = "45 min Power Zone Endurance Ride";
const ENDURANCE_60: &str = "60 min Power Zone Endurance Ride";
const ENDURANCE_90: &str = "90 min Power Zone Endurance Ride";
const POWER_45: &str = "45 min Power Zone Ride";
const POWER_60: &str = "60 min Power Zone Ride";

const W1D1: [PelotonClass; 1] = [class(
    "7a077ff36228426794bd3adc362ca757",
    ENDURANCE_45,
    "Matt Wilpers",
)];
const W1D3: [PelotonClass; 1] = [class(
    "887d10592df041ce808cb483ec05687a",
    ENDURANCE_45,
    "Olivia Amato",
)];
const W1D6: [PelotonClass; 1] = [class(
    "7c55c9f4335a46f2955e2a4827bffa86",
    ENDURANCE_60,
    "Ben Alldis",
)];
const W2D1: [PelotonClass; 1] = [class(
    "9498613d26df46e0ad2f40d262fbce05",
    POWER_45,
    "Denis Morton",
)];
const W2D3: [PelotonClass; 1] = [class(
    "709a359725cf4bffb4cdedb70a6506b0",
    ENDURANCE_45,
    "Olivia Amato",
)];
const W2D6: [PelotonClass; 1] = [class(
    "23cd9015db5947679c321e38dd0082a1",
    ENDURANCE_60,
    "Christine D'Ercole",
)];
const W3D1: [PelotonClass; 1] = [class(
    "b54a1b4ac2924db0bb5a72cf5a540d40",
    POWER_45,
    "Ben Alldis",
)];
const W3D3: [PelotonClass; 1] = [class(
    "9c2466f479684898905f9629b3cc4c83",
    POWER_45,
    "Olivia Amato",
)];
const W3D6: [PelotonClass; 1] = [class(
    "d55e8e879dad415d8a3f3935dd1f4b4f",
    ENDURANCE_60,
    "Denis Morton",
)];
const W4D1: [PelotonClass; 1] = [class(
    "0cd72d4b70c54c8e93b5f13e75fee11d",
    ENDURANCE_45,
    "Christine D'Ercole",
)];
const W4D3: [PelotonClass; 1] = [class(
    "ed1fe2a5e2344dacb2f9bd9984d9ca83",
    ENDURANCE_45,
    "Matt Wilpers",
)];
/// The one class the operator's account cannot start.
const W4D6: [PelotonClass; 1] = [unavailable(
    "c67ec9512f954169acd9df4c95010e49",
    ENDURANCE_60,
    "Denis Morton",
)];
const W5D1: [PelotonClass; 1] = [class(
    "9cae0c2dfe234c529db4da028ff4addd",
    POWER_45,
    "Matt Wilpers",
)];
const W5D3: [PelotonClass; 1] = [class(
    "c2c9fff7966e4743b162b5cc426ad3e7",
    POWER_45,
    "Denis Morton",
)];
const W5D6: [PelotonClass; 1] = [class(
    "5f660f9700ec47599b51dead06fd2a53",
    ENDURANCE_60,
    "Ben Alldis",
)];
const W6D1: [PelotonClass; 1] = [class(
    "062920dde7574be3a5a32628bb11d10c",
    POWER_45,
    "Christine D'Ercole",
)];
const W6D3: [PelotonClass; 1] = [class(
    "b119477c055044458b155d257ebd1bf8",
    POWER_45,
    "Matt Wilpers",
)];
const W6D6: [PelotonClass; 1] = [class(
    "ae5058e68cf045058bbf405b3e115dda",
    POWER_60,
    "Christine D'Ercole",
)];
const W7D1: [PelotonClass; 1] = [class(
    "251e957464f74530937782a6080eecf9",
    "45 min Power Zone Max Ride",
    "Ben Alldis",
)];
const W7D3: [PelotonClass; 1] = [class(
    "57af0cb0dfb44abba73af9798e312d2d",
    POWER_45,
    "Christine D'Ercole",
)];
const W7D6: [PelotonClass; 1] = [class(
    "597a32a0c58a4625b5d9299daffb2e05",
    ENDURANCE_90,
    "Matt Wilpers",
)];
const W8D1: [PelotonClass; 1] = [class(
    "5833bec716724236bd9d12730ff29776",
    ENDURANCE_45,
    "Christine D'Ercole",
)];
const W8D3: [PelotonClass; 1] = [class(
    "a85ab401308f42268394273971f5468c",
    ENDURANCE_45,
    "Denis Morton",
)];
/// **Two classes, one session.** The warm-up ride is shipped separately because
/// the test class carries none of its own.
const W8D6: [PelotonClass; 2] = [
    class(
        "f3474128dec54bbcb7f3775161e4f45e",
        "10 min FTP Warm Up Ride",
        "Matt Wilpers",
    ),
    class(
        "67578c4e666046469a20987ccf70ee5f",
        "20 min FTP Test Ride",
        "Matt Wilpers",
    ),
];

/// Every session of *Peak Your Power Zones*, and what realises it.
pub const PEAK_YOUR_POWER_ZONES: [MappedSession; 24] = [
    MappedSession {
        week: 1,
        day: 1,
        classes: &W1D1,
    },
    MappedSession {
        week: 1,
        day: 3,
        classes: &W1D3,
    },
    MappedSession {
        week: 1,
        day: 6,
        classes: &W1D6,
    },
    MappedSession {
        week: 2,
        day: 1,
        classes: &W2D1,
    },
    MappedSession {
        week: 2,
        day: 3,
        classes: &W2D3,
    },
    MappedSession {
        week: 2,
        day: 6,
        classes: &W2D6,
    },
    MappedSession {
        week: 3,
        day: 1,
        classes: &W3D1,
    },
    MappedSession {
        week: 3,
        day: 3,
        classes: &W3D3,
    },
    MappedSession {
        week: 3,
        day: 6,
        classes: &W3D6,
    },
    MappedSession {
        week: 4,
        day: 1,
        classes: &W4D1,
    },
    MappedSession {
        week: 4,
        day: 3,
        classes: &W4D3,
    },
    MappedSession {
        week: 4,
        day: 6,
        classes: &W4D6,
    },
    MappedSession {
        week: 5,
        day: 1,
        classes: &W5D1,
    },
    MappedSession {
        week: 5,
        day: 3,
        classes: &W5D3,
    },
    MappedSession {
        week: 5,
        day: 6,
        classes: &W5D6,
    },
    MappedSession {
        week: 6,
        day: 1,
        classes: &W6D1,
    },
    MappedSession {
        week: 6,
        day: 3,
        classes: &W6D3,
    },
    MappedSession {
        week: 6,
        day: 6,
        classes: &W6D6,
    },
    MappedSession {
        week: 7,
        day: 1,
        classes: &W7D1,
    },
    MappedSession {
        week: 7,
        day: 3,
        classes: &W7D3,
    },
    MappedSession {
        week: 7,
        day: 6,
        classes: &W7D6,
    },
    MappedSession {
        week: 8,
        day: 1,
        classes: &W8D1,
    },
    MappedSession {
        week: 8,
        day: 3,
        classes: &W8D3,
    },
    MappedSession {
        week: 8,
        day: 6,
        classes: &W8D6,
    },
];

/// What realises the session at this week and day, if this build knows one.
#[must_use]
pub fn session(week: u8, day: u8) -> Option<&'static MappedSession> {
    PEAK_YOUR_POWER_ZONES
        .iter()
        .find(|mapped| mapped.week() == week && mapped.day() == day)
}
