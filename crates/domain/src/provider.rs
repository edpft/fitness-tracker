//! Who provides what, and which part of it we took.
//!
//! **Provider is a relation, not a rung.** The operator, 2026-09-06:
//!
//! > "Peloton and SBS *are* **providers** of **programmes** in the world outside
//! > of this tool. However, within our tool, those same **programmes** are
//! > **providers** of **mesocycles**."
//!
//! So the same word is true twice, at two levels, and neither use is a mistake:
//!
//! ```text
//! Stronger By Science  provides  Squat 2x Int            → provides mesocycles
//! Peloton              provides  Build Your Power Zones  → provides mesocycles
//! ```
//!
//! What this module holds is the upper relation — an external programme and who
//! published it — together with which of its microcycles a mesocycle took. The
//! lower one is not a stored fact but an act: asking a programme for a shape,
//! which is `cycling::shape` and decision 0036.
//!
//! **Not `cycling`'s, though that is where it started.** A `PublishedMicrocycle`
//! carried "which microcycle of which programme" from the day the cycling side
//! was authored, and the gym side carried nothing at all — it stored
//! `template = "sbs"`, which names the publisher of one chart and not the
//! programme. Both disciplines take mesocycles from external programmes, so the
//! record of that sits above both.

use std::fmt;

use crate::{
    gym::sequence::{NonEmpty, TooShort},
    newtype::string_name,
};

/// The longest a published programme's name may be.
///
/// A terminal line, not a rule about naming: the name is printed beside a date
/// and a microcycle selection, and something longer than this wraps. Nothing
/// downstream depends on the value.
pub const MAX_PROGRAMME: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidProgrammeName {
    #[error("a programme's name must not be empty")]
    Empty,
    #[error("a programme's name must be at most {MAX_PROGRAMME} characters, and this is {length}")]
    TooLong { length: usize },
    #[error("a programme's name must be one line of printable text")]
    NotPrintable,
}

/// What a published programme is called: *Squat 2x Int*, *Peak Your Power
/// Zones*.
///
/// **The publisher's word, not ours.** It was this tool's own identity for an
/// authored programme until 2026-09-06, when identity moved up to the plan
/// ([`PlanName`](crate::plan::PlanName)) and the only names left below it were
/// the ones somebody else chose. *Power Zone Build* being wrong here was a
/// transcription error rather than a naming decision, which is the difference.
///
/// **Free text, deliberately.** There is no catalogue of published programmes to
/// validate against, and inventing one would refuse the next programme the
/// operator wants to run. The rules that do exist are the ones a label has to
/// satisfy to be comparable at all: surrounding whitespace is trimmed rather
/// than rejected, so that two transcriptions of one title cannot become two
/// programmes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProgrammeName(String);

impl TryFrom<String> for ProgrammeName {
    type Error = InvalidProgrammeName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(InvalidProgrammeName::Empty);
        }
        let length = trimmed.chars().count();
        if length > MAX_PROGRAMME {
            return Err(InvalidProgrammeName::TooLong { length });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(InvalidProgrammeName::NotPrintable);
        }
        Ok(Self(trimmed.to_owned()))
    }
}

string_name!(ProgrammeName, InvalidProgrammeName);

/// The longest a provider's name may be.
///
/// A terminal line rather than a rule about naming, as [`ProgrammeName`]'s is.
pub const MAX_PROVIDER: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidProvider {
    #[error("a provider's name must not be empty")]
    Empty,
    #[error("a provider's name must be at most {MAX_PROVIDER} characters, and this is {length}")]
    TooLong { length: usize },
    #[error("a provider's name must be one line of printable text")]
    NotPrintable,
}

/// Who published a programme: *Stronger By Science*, *Peloton*.
///
/// **Free text, as [`ProgrammeName`] is.** There is no list of publishers to
/// validate against and inventing one would refuse the next programme the
/// operator wants to run.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Provider(String);

impl TryFrom<String> for Provider {
    type Error = InvalidProvider;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(InvalidProvider::Empty);
        }
        let length = trimmed.chars().count();
        if length > MAX_PROVIDER {
            return Err(InvalidProvider::TooLong { length });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(InvalidProvider::NotPrintable);
        }
        Ok(Self(trimmed.to_owned()))
    }
}

string_name!(Provider, InvalidProvider);

/// A programme published outside this tool: *Squat 2x Int*, *Build Your Power
/// Zones*.
///
/// **The provider travels with it**, because "Build Your Power Zones" alone does
/// not say who published it and two providers may one day use one title. It is
/// not on the plan or on the discipline: a plan's cycling side could take one
/// mesocycle from Peloton and the next from somewhere else, and nothing in the
/// model should have to be rewritten when it does.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExternalProgramme {
    provider: Provider,
    name: ProgrammeName,
}

impl ExternalProgramme {
    pub const fn new(provider: Provider, name: ProgrammeName) -> Self {
        Self { provider, name }
    }

    pub const fn provider(&self) -> &Provider {
        &self.provider
    }

    pub const fn name(&self) -> &ProgrammeName {
        &self.name
    }
}

impl fmt::Display for ExternalProgramme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Which microcycles of which external programme a mesocycle is.
///
/// **The published numbering, never ours.** An answer of µ1-2-4-5 keeps the four
/// numbers the programme itself uses, so the third microcycle here says 4. That
/// is the way back to what was not chosen: a re-authoring that wants the week's
/// third session knows which microcycle of which programme to ask for.
///
/// **Ordered and without repeats**, because they are a selection from one
/// programme rather than a sequence of arbitrary weeks. µ1-2-4-5 is a subset in
/// its own order; µ1-1-2 is not a selection anybody made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvidedFrom {
    programme: ExternalProgramme,
    microcycles: NonEmpty<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidProvision {
    #[error("a mesocycle is at least one microcycle of the programme it came from")]
    NoMicrocycles,
    #[error("a published programme's microcycles count from one, so there is no microcycle 0")]
    ZeroMicrocycle,
    #[error("microcycle {microcycle} is taken twice, and a selection takes each week once")]
    RepeatedMicrocycle { microcycle: u32 },
}

impl From<TooShort> for InvalidProvision {
    fn from(_: TooShort) -> Self {
        Self::NoMicrocycles
    }
}

impl ProvidedFrom {
    /// # Errors
    ///
    /// [`InvalidProvision`] for no microcycles, a microcycle numbered zero, or
    /// one taken twice.
    pub fn new(
        programme: ExternalProgramme,
        microcycles: Vec<u32>,
    ) -> Result<Self, InvalidProvision> {
        let mut seen: Vec<u32> = Vec::with_capacity(microcycles.len());
        for microcycle in &microcycles {
            if *microcycle == 0 {
                return Err(InvalidProvision::ZeroMicrocycle);
            }
            if seen.contains(microcycle) {
                return Err(InvalidProvision::RepeatedMicrocycle {
                    microcycle: *microcycle,
                });
            }
            seen.push(*microcycle);
        }
        Ok(Self {
            programme,
            microcycles: NonEmpty::new(microcycles)?,
        })
    }

    pub const fn programme(&self) -> &ExternalProgramme {
        &self.programme
    }

    pub fn microcycles(&self) -> impl Iterator<Item = u32> + '_ {
        self.microcycles.iter().copied()
    }
}

impl fmt::Display for ProvidedFrom {
    /// `micros 1-2-4-5`, and `micro 5` for one.
    ///
    /// The programme is not printed here: it is a column of its own everywhere
    /// this appears, and repeating it would print *Build Your Power Zones* twice
    /// on one line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let numbers: Vec<String> = self.microcycles.iter().map(ToString::to_string).collect();
        if numbers.len() == 1 {
            write!(f, "micro {}", numbers.join("-"))
        } else {
            write!(f, "micros {}", numbers.join("-"))
        }
    }
}
