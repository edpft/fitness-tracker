//! How long a free slot is, as the operator says it.

/// Whether a slot has room for the longer session of a week or only the
/// shorter. How long exactly does not matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlotLength {
    Shorter,
    Longer,
}

impl SlotLength {
    pub const ALL: &'static [Self] = &[Self::Shorter, Self::Longer];

    /// The stable key. Persisted, so it outlives a rename.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shorter => "shorter",
            Self::Longer => "longer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is neither longer nor shorter")]
pub struct UnknownSlotLength {
    value: String,
}

impl TryFrom<String> for SlotLength {
    type Error = UnknownSlotLength;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .iter()
            .find(|length| length.as_str() == value)
            .copied()
            .ok_or(UnknownSlotLength { value })
    }
}

impl TryFrom<&str> for SlotLength {
    type Error = UnknownSlotLength;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
    }
}

impl std::str::FromStr for SlotLength {
    type Err = UnknownSlotLength;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl std::fmt::Display for SlotLength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
