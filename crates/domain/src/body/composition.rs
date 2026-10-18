//! What a body is made of, as each source states it (#275).
//!
//! **One table for every source, and one vocabulary.** A Body Scan, an InBody
//! and a figure typed into a spreadsheet all say what the body is made of, so
//! their composition shares a shape; what tells them apart is the method each
//! row names, and § 6 keeps their rows in separate series.
//!
//! **A component is named for what was measured.** InBody's skeletal muscle
//! mass (35.5 kg) and the Body Scan's muscle mass (62.74 kg, of 83.6) are not
//! one quantity read two ways, and neither are InBody's segmental lean mass and
//! the Body Scan's fat-free mass: the Body Scan's five segments sum to its whole
//! body, InBody's do not. So each is its own component, never folded into a
//! generic "muscle" or "lean".
//!
//! **Each amount is as the source stated it**: a mass, a volume or a
//! percentage of the weigh-in's mass (operator, 2026-10-16). Normalisation
//! converts none into another; the analytical layer reads the linked weigh-in
//! where it needs to.

/// A thing the body is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Component {
    Fat,
    FatFree,
    /// The Body Scan's muscle mass.
    Muscle,
    /// InBody's skeletal muscle mass.
    SkeletalMuscle,
    /// InBody's lean mass, by segment.
    Lean,
    Water,
    ExtracellularWater,
    IntracellularWater,
    Bone,
    Protein,
    Minerals,
}

impl Component {
    pub const ALL: [Self; 11] = [
        Self::Fat,
        Self::FatFree,
        Self::Muscle,
        Self::SkeletalMuscle,
        Self::Lean,
        Self::Water,
        Self::ExtracellularWater,
        Self::IntracellularWater,
        Self::Bone,
        Self::Protein,
        Self::Minerals,
    ];

    /// The component a persisted key names.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|component| component.as_str() == key)
    }

    /// The stable key. Persisted.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fat => "fat",
            Self::FatFree => "fat-free",
            Self::Muscle => "muscle",
            Self::SkeletalMuscle => "skeletal-muscle",
            Self::Lean => "lean",
            Self::Water => "water",
            Self::ExtracellularWater => "extracellular-water",
            Self::IntracellularWater => "intracellular-water",
            Self::Bone => "bone",
            Self::Protein => "protein",
            Self::Minerals => "minerals",
        }
    }
}
