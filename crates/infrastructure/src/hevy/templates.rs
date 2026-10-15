//! Templates Hevy does not have, created on the operator's account (#455).
//!
//! Until now he duplicated the nearest template in the app. Hevy's API creates
//! one but cannot duplicate, update or delete one, so each is made from the
//! template it stands in for: that template's muscles, with its own title, type
//! and equipment. A template created in error has to be removed in the app,
//! which is why creating one is idempotent by title.
//!
//! The ids Hevy gives them are this account's, so they are compiled into
//! [`super::mapping`] and [`super::writable`] once created, exactly as the
//! operator's earlier custom templates are.

use application::DeliveryError;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};

use super::destination::HevyRoutines;

const TEMPLATES_ENDPOINT: &str = "/v1/exercise_templates";

/// Hevy's own maximum page size for templates.
const PAGE_SIZE: u32 = 100;

/// How many pages to walk looking for a title. Hevy's library is a few hundred
/// templates, so this is generous.
const PAGE_LIMIT: u32 = 20;

/// One template to create, and the one it is made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomTemplate {
    pub title: &'static str,
    /// The template whose muscles it takes.
    pub from: &'static str,
    /// Hevy's `exercise_type`: what a set on it records.
    pub exercise_type: &'static str,
    /// Hevy's `equipment_category`.
    pub equipment: &'static str,
}

/// Every template the vocabulary needs and Hevy does not have.
pub const CUSTOM: &[CustomTemplate] = &[
    // Hevy's Front Lever Raise records weight added, so a band's assistance
    // could only go in the notes (2026-10-12).
    CustomTemplate {
        title: "Front Lever Raise (Band)",
        from: "30F03BF0",
        exercise_type: "bodyweight_assisted_reps",
        equipment: "resistance_band",
    },
    // Recorded as Skullcrusher (Barbell), though the bar path differs.
    CustomTemplate {
        title: "Lying Overhead Triceps Extension (EZ Bar)",
        from: "875F585F",
        exercise_type: "weight_reps",
        equipment: "barbell",
    },
];

/// What became of one custom template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Made {
    /// It was already on the account.
    Existing { title: &'static str, id: String },
    Created { title: &'static str, id: String },
}

#[derive(Debug, Deserialize)]
struct TemplatePage {
    #[serde(default)]
    exercise_templates: Vec<Template>,
}

#[derive(Debug, Deserialize)]
struct Template {
    id: String,
    title: String,
    #[serde(default)]
    is_custom: bool,
    #[serde(default)]
    primary_muscle_group: Option<String>,
    #[serde(default)]
    secondary_muscle_groups: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CreateTemplate<'a> {
    exercise: NewTemplate<'a>,
}

#[derive(Debug, Serialize)]
struct NewTemplate<'a> {
    title: &'a str,
    exercise_type: &'a str,
    equipment_category: &'a str,
    muscle_group: &'a str,
    other_muscles: &'a [String],
}

impl HevyRoutines {
    /// The custom template with this title, if the account has one.
    async fn custom_titled(&self, title: &str) -> Result<Option<String>, DeliveryError> {
        for page in 1..=PAGE_LIMIT {
            let body: TemplatePage = self
                .read(|client| {
                    client
                        .get(self.url(TEMPLATES_ENDPOINT))
                        .header("api-key", self.api_key())
                        .query(&[
                            ("page", page.to_string()),
                            ("pageSize", PAGE_SIZE.to_string()),
                        ])
                })
                .await?;
            if let Some(found) = body
                .exercise_templates
                .iter()
                .find(|template| template.is_custom && template.title == title)
            {
                return Ok(Some(found.id.clone()));
            }
            if body.exercise_templates.len() < PAGE_SIZE as usize {
                return Ok(None);
            }
        }
        Ok(None)
    }

    async fn template(&self, id: &str) -> Result<Template, DeliveryError> {
        self.read(|client| {
            client
                .get(self.url(&format!("{TEMPLATES_ENDPOINT}/{id}")))
                .header("api-key", self.api_key())
        })
        .await
    }

    /// Create every [`CUSTOM`] template the account does not already have.
    ///
    /// Hevy answers a creation with a number rather than the id its templates
    /// are named by elsewhere, so the id reported is the one the listing gives
    /// the new template afterwards.
    ///
    /// # Errors
    ///
    /// [`DeliveryError`] if Hevy refuses or cannot be reached; templates created
    /// before the failure stay created, and a rerun finds them.
    pub async fn make_custom_templates(&self) -> Result<Vec<Made>, DeliveryError> {
        let mut made = Vec::new();
        for custom in CUSTOM {
            if let Some(id) = self.custom_titled(custom.title).await? {
                made.push(Made::Existing {
                    title: custom.title,
                    id,
                });
                continue;
            }
            let from = self.template(custom.from).await?;
            let muscle = from.primary_muscle_group.as_deref().unwrap_or("other");
            let body = Self::encode(&CreateTemplate {
                exercise: NewTemplate {
                    title: custom.title,
                    exercise_type: custom.exercise_type,
                    equipment_category: custom.equipment,
                    muscle_group: muscle,
                    other_muscles: &from.secondary_muscle_groups,
                },
            })?;
            let _: serde_json::Value = self
                .read(|client| {
                    client
                        .post(self.url(TEMPLATES_ENDPOINT))
                        .header("api-key", self.api_key())
                        .header(CONTENT_TYPE, "application/json")
                        .body(body)
                })
                .await?;
            let id = self.custom_titled(custom.title).await?.ok_or_else(|| {
                DeliveryError::Unreachable {
                    destination: "hevy".to_owned(),
                    message: format!("created {} but cannot find it", custom.title),
                }
            })?;
            made.push(Made::Created {
                title: custom.title,
                id,
            });
        }
        Ok(made)
    }
}
