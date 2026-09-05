//! Which class sits at which microcycle and session, for each published
//! programme.
//!
//! **The skeleton is the operator's and the content is fetched** (decision
//! 0033). Peloton serves class details and does not serve programme structure,
//! so the placements below are given and everything about what a class contains
//! is read from the API at the time it is asked for.
//!
//! **Ordinal, not calendar** (decision 0032): a microcycle and a session
//! position, never a weekday. The programme counts cycles and the scheduler owns
//! the calendar (0018).
//!
//! **Code, not data** (§ 9), for the reason `hevy::mapping` is: a deterministic
//! translation must not be editable without review. The `code` parameter of the
//! share links these came from is deliberately absent — it decodes to two
//! further identifiers, one plausibly the operator's own Peloton user id.
//!
//! *Discover* is not here and should not be added. It is an onboarding
//! programme — seven classes over five days in its first week — and the operator
//! settled on 2026-09-05 that it is not one this tool pulls from.

use super::mapping::PEAK_YOUR_POWER_ZONES;

/// One class, placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub microcycle: u8,
    pub session: u8,
    pub class_id: &'static str,
}

const fn at(microcycle: u8, session: u8, class_id: &'static str) -> Placement {
    Placement {
        microcycle,
        session,
        class_id,
    }
}

/// A published programme, as a set of placements.
#[derive(Debug, Clone, Copy)]
pub struct Skeleton {
    name: &'static str,
    placements: &'static [Placement],
}

impl Skeleton {
    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn placements(&self) -> &'static [Placement] {
        self.placements
    }

    /// Every microcycle this programme runs, in order.
    #[must_use]
    pub fn microcycles(&self) -> Vec<u8> {
        let mut seen: Vec<u8> = self.placements.iter().map(|at| at.microcycle).collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    }

    /// Every session position this programme uses, in order.
    #[must_use]
    pub fn sessions(&self) -> Vec<u8> {
        let mut seen: Vec<u8> = self.placements.iter().map(|at| at.session).collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    }
}

/// *Boost Your Base* — eight microcycles of three, and no FTP test of its own.
const BOOST_YOUR_BASE_PLACEMENTS: [Placement; 24] = [
    at(1, 1, "0bc8a790d8ca49cc8355cc7411842ca9"),
    at(1, 2, "5428765b60284c3ab0353420d7487298"),
    at(1, 3, "7806028d47314d71a156271cbe70825a"),
    at(2, 1, "f4c20aa732a44e8da134cfa4c87e2932"),
    at(2, 2, "8a9256c468f945079ab9c5df02e42340"),
    at(2, 3, "e122a7a91e8246e8b7bd3e57ac699452"),
    at(3, 1, "0b0e2b952cb142be99cba6e7847e838d"),
    at(3, 2, "1b5642f031e649dba52b297e26ea9568"),
    at(3, 3, "ca84d5a2c9244b0b8e7bc26432aedc16"),
    at(4, 1, "8f5703814c4c47aca18c8d158f72d4ca"),
    at(4, 2, "3ab49db89d564099a516b19055e43f48"),
    at(4, 3, "3f536cc3322c4b329de2a589bb4b2c4d"),
    at(5, 1, "d98c23f64c6a4607a93eabff167f10fc"),
    at(5, 2, "84bf21c2dc88448dbf20851d2d35336e"),
    at(5, 3, "b808508d4f80434392c4fd7a139a835a"),
    at(6, 1, "7a062d47bdfc4976acb6f3a8af4f69ac"),
    at(6, 2, "dd435e03638b44b9bc1404f8d380f079"),
    at(6, 3, "6a3c724c18734fcc990fe43480818836"),
    at(7, 1, "ecfb377e69164142b43e6b223522b98f"),
    at(7, 2, "1cdb8d79f8e34e738bb73a1aa4dea048"),
    at(7, 3, "aab6d338d6e04d6780873bdabafc1d45"),
    at(8, 1, "f9671bfa402540e19140c9d832838744"),
    at(8, 2, "6aac82dba3d241cfbbae25aaac267e9c"),
    at(8, 3, "d7dc5f2e61324a00968d7f84c07ec211"),
];

/// *Power Zone Build* — five microcycles of three. The last session is two
/// classes, because the test carries no warm-up of its own.
const POWER_ZONE_BUILD_PLACEMENTS: [Placement; 16] = [
    at(1, 1, "9f8f3af689cc4f0db9afa013d4676ed6"),
    at(1, 2, "44867a5486184a09b8ca135a6d8c7494"),
    at(1, 3, "e825140788a84d31b3948419e50bedb5"),
    at(2, 1, "7fa9796be8484c9987122d357de25fc7"),
    at(2, 2, "49c2c7626aba4effa39b53b85c0e16f6"),
    at(2, 3, "d3d85447e9d14ea29344a002823a82ca"),
    at(3, 1, "a5f95a660f5b4a84ac6a86aa4468ea1d"),
    at(3, 2, "daa6ce2d3a454d1c82937fa926a59db4"),
    at(3, 3, "414a518108ea4c5cada00ab9899a9d8d"),
    at(4, 1, "4c2110dd4de74c9b9a4a4e9176501702"),
    at(4, 2, "47ad3764fbfb4774967958874465bee4"),
    at(4, 3, "1b768ee376c546ae92493d8301eeec85"),
    at(5, 1, "725d618516674f7581d4d566fe3f0655"),
    at(5, 2, "4355cbf8734648c1a26c2f6d354035c5"),
    at(5, 3, "1eabf70b20744f48b99259f93889ced5"),
    at(5, 3, "4d302bef49574118a071269bed38bd30"),
];

pub const BOOST_YOUR_BASE: Skeleton = Skeleton {
    name: "Boost Your Base",
    placements: &BOOST_YOUR_BASE_PLACEMENTS,
};

pub const POWER_ZONE_BUILD: Skeleton = Skeleton {
    name: "Power Zone Build",
    placements: &POWER_ZONE_BUILD_PLACEMENTS,
};

/// *Peak Your Power Zones*, derived from the table that already holds it.
///
/// **Not duplicated.** `mapping::PEAK_YOUR_POWER_ZONES` carries Peak's class ids
/// with the weekday numbering they were transcribed under; this converts days
/// 1, 3 and 6 into session positions 1, 2 and 3. A day that is none of those
/// would be dropped, which cannot happen for a table this module can see.
#[must_use]
pub fn peak_your_power_zones() -> Vec<Placement> {
    PEAK_YOUR_POWER_ZONES
        .iter()
        .filter_map(|mapped| {
            let session = match mapped.day() {
                1 => 1,
                3 => 2,
                6 => 3,
                _ => return None,
            };
            Some((mapped.week(), session, mapped.classes()))
        })
        .flat_map(|(microcycle, session, classes)| {
            classes
                .iter()
                .map(move |class| at(microcycle, session, class.class_id()))
        })
        .collect()
}

/// How many microcycles *Power Zone Build* runs, and so which of them the test
/// programme was copied from: its last.
const BUILD_MICROCYCLES: u8 = 5;

/// *Power Zone test* — one microcycle, and a programme in its own right.
///
/// **A test microcycle is a standalone programme, not a microcycle borrowed
/// from another one.** The operator, 2026-09-05:
///
/// > "my idea was to copy the 5th microcycle of Build into a standalone Power
/// > Zone test programme. this is also what I intended with the standalone SBS
/// > test. so, the tool shouldn't be able to confuse it with the actual 5th
/// > microcycle of the Build programme because, even though it's exactly the
/// > same classes, it's a separate thing."
///
/// It is the same rule the gym already follows: `autumn-entry-test` is a `test`
/// programme of its own and not "SBS µ4". Without it the autumn authored *Power
/// Zone Build µ5* twice — once as the opening test week and again five weeks
/// later as the deload closing the first mesocycle — and nothing in the record
/// could tell the two apart.
///
/// **Derived from Build rather than restated**, so one list of class ids is
/// kept in step rather than two. What makes it a separate thing is its name and
/// its own numbering, which is exactly what the operator said is separate about
/// it; the classes really are the same classes.
#[must_use]
pub fn power_zone_test() -> Vec<Placement> {
    POWER_ZONE_BUILD_PLACEMENTS
        .iter()
        .filter(|placed| placed.microcycle == BUILD_MICROCYCLES)
        .map(|placed| at(1, placed.session, placed.class_id))
        .collect()
}

/// What the standalone test programme is called.
///
/// Named here because it is the name the authored record carries, and a name
/// spelled twice is a name that drifts.
pub const POWER_ZONE_TEST: &str = "Power Zone test";

/// Every programme this build can pull a mesocycle from.
///
/// Peak and the test programme are absent from this list and reached through
/// [`peak_your_power_zones`] and [`power_zone_test`], because their placements
/// are derived rather than stated. A caller wanting all of them joins them.
pub const SKELETONS: [Skeleton; 2] = [BOOST_YOUR_BASE, POWER_ZONE_BUILD];
