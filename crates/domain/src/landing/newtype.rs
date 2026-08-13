//! The conversions the value types share.
//!
//! Each type owns exactly one thing: which values it accepts. Everything else
//! — borrowing it, printing it, reading it back from text — is identical
//! across a family and is generated here rather than written out five times
//! with five chances to differ.

/// Implement the shared surface of a string-backed name.
///
/// Expects a tuple struct whose single field is a `String`, and an existing
/// `TryFrom<String>` carrying the validation. `TryFrom<String>` is the
/// implementation rather than `TryFrom<&str>` because every one of these types
/// owns a `String`: taking the caller's allocation is free, and copying it is
/// not. The borrowed forms delegate.
macro_rules! string_name {
    ($name:ident, $error:ty) => {
        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = $error;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::try_from(value.to_owned())
            }
        }

        /// So that `str::parse` works, and so that anything driven by
        /// `FromStr` — clap's value parsers, `serde`'s string forms — can
        /// build one without knowing it exists.
        impl ::std::str::FromStr for $name {
            type Err = $error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::try_from(value.to_owned())
            }
        }
    };
}

/// Implement the shared surface of an instant.
///
/// Expects a `Copy` tuple struct whose single field is a `jiff::Timestamp`.
/// Every instant here is infallible from a timestamp and fallible from text,
/// and RFC 3339 is the only text form any of them accepts.
macro_rules! instant {
    ($name:ident) => {
        impl $name {
            pub fn as_timestamp(self) -> Timestamp {
                self.0
            }
        }

        impl From<Timestamp> for $name {
            fn from(at: Timestamp) -> Self {
                Self(at)
            }
        }

        impl From<$name> for Timestamp {
            fn from(at: $name) -> Self {
                at.0
            }
        }

        impl TryFrom<&str> for $name {
            type Error = InvalidTimestamp;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                parse(value).map(Self)
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = InvalidTimestamp;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                parse(value).map(Self)
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

pub(crate) use {instant, string_name};
