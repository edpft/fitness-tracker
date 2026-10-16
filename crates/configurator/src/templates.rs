//! Hevy templates for exercises Hevy does not have (#455).

use application::DeliveryError;
use infrastructure::hevy::templates::Made;

use crate::{
    access::{self, AccessError},
    catalogue::KnownSource,
};

/// One custom template, and whether this run created it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MadeTemplate {
    pub title: &'static str,
    pub id: String,
    pub created: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error(transparent)]
    Access(#[from] AccessError),
    #[error(transparent)]
    Hevy(#[from] DeliveryError),
}

/// Create on the operator's account every custom template it lacks.
///
/// # Errors
///
/// [`TemplateError`] if there is no key, or Hevy refuses or cannot be reached.
pub async fn make_custom(
    known: &'static KnownSource,
    credentials: &crate::Credentials,
) -> Result<Vec<MadeTemplate>, TemplateError> {
    let hevy = access::hevy_routines(known, credentials)?;
    Ok(hevy
        .make_custom_templates()
        .await?
        .into_iter()
        .map(|made| match made {
            Made::Existing { title, id } => MadeTemplate {
                title,
                id,
                created: false,
            },
            Made::Created { title, id } => MadeTemplate {
                title,
                id,
                created: true,
            },
        })
        .collect())
}
