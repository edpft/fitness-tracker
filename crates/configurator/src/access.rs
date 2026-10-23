//! The adapters a session is delivered through, from a resolved credential.
//!
//! **Not a stream's walks**, which [`crate::wiring`] composes: delivering lands
//! nothing, so these are reached by the source a discipline delivers to rather
//! than by a stream it collects.

use application::DeliveryError;
use infrastructure::{
    HevyRoutines,
    peloton::{
        PelotonStack,
        auth::{PelotonAuth, PelotonCredentials},
        class::PelotonClasses,
        stack::gateway_for,
    },
};

use crate::{
    catalogue::{KnownSource, PELOTON, source},
    config::{ConfigError, SourceAccess, source_access_for},
};

/// Why a source could not be reached for delivery.
#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("this build does not connect to {source_name}")]
    Unconnected { source_name: &'static str },
    /// The catalogue and the credential it resolved disagree about the kind,
    /// which is a mistake in this build rather than in the invocation.
    #[error("{source_name} is reached with {wanted}")]
    WrongKind {
        source_name: String,
        wanted: &'static str,
    },
    #[error(transparent)]
    Delivery(#[from] DeliveryError),
}

/// The Peloton adapters, from a resolved login.
///
/// **One place composes both**, because they need the same login and the same
/// cached token, and building them twice from two places is how one of them
/// ends up without the cache.
///
/// **The login is resolved the way every other source's is**: the environment
/// first, then the store `fitness credentials peloton` writes to. It read
/// `PELOTON_EMAIL` and `PELOTON_PASSWORD` out of the environment and nowhere
/// else until #184, which made the store invisible to `cycling next` — and on
/// the installed build the store is the only place a credential is kept.
///
/// # Errors
///
/// [`AccessError`] if neither the environment nor the store holds a Peloton
/// login.
pub fn peloton(credentials: &infrastructure::Credentials) -> Result<Peloton, AccessError> {
    let access = access(credentials)?;
    let classes = PelotonClasses::new(access.base_url.clone(), access.auth());
    // **The stack's root follows the API's** (#196), so a run pointed at a
    // stub delivers to the stub rather than to the operator's account.
    let stack = PelotonStack::new(gateway_for(&access.base_url), access.auth());
    Ok(Peloton { classes, stack })
}

/// Peloton, logged in: its classes, and the stack a ride is put in.
pub struct Peloton {
    pub(crate) classes: PelotonClasses,
    pub(crate) stack: PelotonStack,
}

/// A resolved Peloton login, and the root it is exchanged at.
struct Access {
    base_url: String,
    auth_base_url: String,
    credentials: PelotonCredentials,
}

impl Access {
    /// An authenticator, composed where every other one is.
    ///
    /// **One per adapter rather than one shared**: each reads the token the
    /// other cached, so the second costs no login and neither adapter holds the
    /// other's state.
    fn auth(&self) -> PelotonAuth {
        crate::wiring::peloton_auth(self.auth_base_url.clone(), self.credentials.clone())
    }
}

fn access(credentials: &infrastructure::Credentials) -> Result<Access, AccessError> {
    let known = source(PELOTON).ok_or(AccessError::Unconnected {
        source_name: PELOTON,
    })?;

    // The catalogue says Peloton authenticates with a login, so the other two
    // arms are a contradiction rather than a case: they would mean its entry
    // and the call that read its credential disagree.
    let SourceAccess::EmailPassword {
        base_url,
        auth_base_url,
        email,
        password,
        ..
    } = source_access_for(known, None, credentials)?
    else {
        return Err(AccessError::WrongKind {
            source_name: PELOTON.to_owned(),
            wanted: "a login",
        });
    };

    Ok(Access {
        base_url,
        auth_base_url,
        credentials: PelotonCredentials::new(email, password),
    })
}

/// Hevy's routines, where a gym session is put.
///
/// **The destination is passed in rather than named here.** It used to be the
/// string "hevy", which was honest while there was one of them and became a
/// scaling question the moment a second discipline was contemplated: a
/// discipline knows its own sink, so `gym next` hands its one over and the flat
/// command names the build's only one. Neither guesses.
///
/// # Errors
///
/// [`AccessError`] if there is no key, or the catalogue says the source is
/// reached some other way.
pub fn hevy_routines(
    known: &'static KnownSource,
    credentials: &infrastructure::Credentials,
) -> Result<HevyRoutines, AccessError> {
    let base_url = std::env::var(known.base_url_variable())
        .unwrap_or_else(|_| known.default_base_url().to_owned());
    let access = SourceAccess::resolve(
        known,
        base_url,
        std::env::var(known.api_key_variable()),
        credentials.credential(known.name()),
    )?;

    // `resolve` above builds the key-based kind, so the other arm is a
    // contradiction rather than a case: it would mean this source's catalogue
    // entry and the call that read its credential disagree.
    let SourceAccess::ApiKey {
        base_url, api_key, ..
    } = access
    else {
        return Err(AccessError::WrongKind {
            source_name: known.name().to_owned(),
            wanted: "an API key",
        });
    };

    Ok(HevyRoutines::new(base_url, api_key)?)
}

#[cfg(test)]
mod tests {
    use infrastructure::peloton::stack::{GATEWAY, gateway_for};

    use crate::catalogue::{Credential, PELOTON, source};

    /// **The stack's composed default is Peloton's gateway, and any other
    /// root is where the stack goes** (#196). A stub cannot catch a wrong
    /// default, so the default is pinned here; and a run with
    /// `PELOTON_API_BASE_URL` pointed at a stub delivers to that stub.
    #[test]
    fn the_stack_follows_the_api_root() {
        let known = source(PELOTON).expect("peloton is a known source");
        assert_eq!(
            gateway_for(known.default_base_url()),
            "https://gql-graphql-gateway.prod.k8s.onepeloton.com/graphql"
        );
        assert_eq!(gateway_for(known.default_base_url()), GATEWAY);
        assert_eq!(
            gateway_for("http://127.0.0.1:9/"),
            "http://127.0.0.1:9/graphql",
            "a stub's root answers the stack itself"
        );
        assert_ne!(
            gateway_for("http://127.0.0.1:9"),
            GATEWAY,
            "never the real account"
        );
    }

    /// **The roots `plan` used to compile in are the catalogue's.**
    ///
    /// `AUTH_BASE` and `API_BASE` were private constants there until #184, and a
    /// stub-based test could not have seen them disagree with the entry every
    /// other Peloton command reads — nothing pointed both at the same stub.
    /// Pinning the literals is what makes deleting them safe.
    #[test]
    fn peloton_is_reached_at_the_roots_this_module_used_to_hold() {
        let known = source(PELOTON).expect("peloton is a known source");
        assert_eq!(known.default_base_url(), "https://api.onepeloton.com");

        let Credential::EmailPassword {
            default_auth_base_url,
        } = known.credential()
        else {
            panic!("peloton is reached with a login");
        };
        assert_eq!(default_auth_base_url, "https://auth.onepeloton.com");
    }
}
