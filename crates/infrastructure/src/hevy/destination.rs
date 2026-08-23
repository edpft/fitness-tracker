//! Hevy as a place a prescription can be put.
//!
//! **A renderer that returns a receipt.** Everything about *what* the session
//! instructs was settled before this adapter was called; what it adds is a
//! rendering ([`super::routine`]) and the identity the source gives what it is
//! handed. That identity is the only thing that crosses back over the port.
//!
//! ## Created, never updated
//!
//! Nothing here calls `PUT`. An issued prescription is written once and never
//! rewritten (§ 12), a reissue is a different prescription, and so every
//! delivery is a `POST`. That the source also publishes no `DELETE` — for a
//! routine or for a folder — and retires the id of anything removed by hand is
//! an agreement with that rule rather than the reason for it. The consequence is
//! the one worth having: a routine id names exactly one issued session, which is
//! what makes it a key a performed workout can later be matched on. The record
//! shows the alternative — of the 8 landed workouts carrying a routine id, 5
//! carry the *same* one, because that routine was rewritten in place.
//!
//! ## The folder
//!
//! One per programme, resolved by title on the way past: found if it exists,
//! created if it does not. That is the only concession to the app's own shape in
//! this module, and it is a rendering decision — where a reader looks for the
//! session — rather than anything the domain knows about.

use std::{sync::OnceLock, time::Duration};

use application::{
    Deliverable, Delivered, DeliveryError, DeliveryReference, DestinationName,
    PrescriptionDestination,
};
use reqwest::{Client, StatusCode, header::CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use super::routine::{CreateRoutine, render};

/// Where routines are created.
pub const ROUTINES_ENDPOINT: &str = "/v1/routines";
/// Where routine folders are listed and created.
pub const FOLDERS_ENDPOINT: &str = "/v1/routine_folders";

/// The source caps a page at 10, exactly as it does for the events feed.
const PAGE_SIZE: u32 = 10;

/// How many folder pages to walk before giving up looking for one by name.
///
/// A bound rather than a walk to exhaustion: a folder list that never ends is a
/// source fault, and creating a duplicate folder is a better failure than
/// looping. Ten pages is a hundred folders.
const FOLDER_PAGE_LIMIT: u32 = 10;

/// The name this destination is recorded under.
const NAME: &str = "hevy";

/// Hevy, as somewhere a session can be sent.
#[derive(Debug)]
pub struct HevyRoutines {
    client: OnceLock<Result<Client, String>>,
    base_url: String,
    api_key: String,
    name: DestinationName,
}

impl HevyRoutines {
    /// # Errors
    ///
    /// [`DeliveryError::Unidentifiable`] if the compiled-in name is not a legal
    /// destination name. Pinned by a test, so it is unreachable in practice.
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, DeliveryError> {
        Ok(Self {
            client: OnceLock::new(),
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            api_key: api_key.into(),
            name: DestinationName::try_from(NAME.to_owned()).map_err(|error| {
                DeliveryError::Unidentifiable {
                    destination: NAME.to_owned(),
                    message: error.to_string(),
                }
            })?,
        })
    }

    /// Built once on first use, for the reason
    /// [`super::client::HevyWorkoutEvents`] gives: constructing a port does no
    /// I/O, so a TLS failure surfaces when the adapter is asked to work rather
    /// than while the composition root is still assembling.
    fn client(&self) -> Result<&Client, DeliveryError> {
        let built = self.client.get_or_init(|| {
            Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| error.to_string())
        });

        built.as_ref().map_err(|detail| DeliveryError::Unreachable {
            destination: NAME.to_owned(),
            message: detail.clone(),
        })
    }

    fn url(&self, endpoint: &str) -> String {
        format!("{}{endpoint}", self.base_url)
    }

    /// The folder for a programme: the one with that title, or a new one.
    async fn folder_for(&self, title: &str) -> Result<Option<i64>, DeliveryError> {
        if let Some(existing) = self.find_folder(title).await? {
            return Ok(Some(existing));
        }
        self.create_folder(title).await
    }

    async fn find_folder(&self, title: &str) -> Result<Option<i64>, DeliveryError> {
        for page in 1..=FOLDER_PAGE_LIMIT {
            let response = self
                .client()?
                .get(self.url(FOLDERS_ENDPOINT))
                .header("api-key", &self.api_key)
                .query(&[
                    ("page", page.to_string()),
                    ("pageSize", PAGE_SIZE.to_string()),
                ])
                .send()
                .await
                .map_err(|error| Self::unreachable(&error.to_string()))?;

            let body: FolderPage = self.read(response).await?;
            if let Some(found) = body
                .routine_folders
                .iter()
                .find(|folder| folder.title == title)
            {
                return Ok(Some(found.id));
            }
            if body.routine_folders.len() < PAGE_SIZE as usize {
                return Ok(None);
            }
        }
        Ok(None)
    }

    async fn create_folder(&self, title: &str) -> Result<Option<i64>, DeliveryError> {
        let response = self
            .client()?
            .post(self.url(FOLDERS_ENDPOINT))
            .header("api-key", &self.api_key)
            .header(CONTENT_TYPE, "application/json")
            .body(Self::encode(&CreateFolder {
                routine_folder: FolderTitle {
                    title: title.to_owned(),
                },
            })?)
            .send()
            .await
            .map_err(|error| Self::unreachable(&error.to_string()))?;

        let created: CreatedFolder = self.read(response).await?;
        Ok(Some(created.routine_folder.id))
    }

    /// Read a successful body, or turn the status into the right error.
    ///
    /// **Unauthorised is `Unreachable`, not a panic and not a silent skip**: a
    /// credential that has been revoked degrades the system (§ 36) and leaves
    /// the prescription exactly where it was.
    async fn read<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, DeliveryError> {
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            let message = if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                format!("{status}: the API key was refused")
            } else {
                format!("{status}: {}", detail.trim())
            };
            return Err(Self::unreachable(&message));
        }

        // Parsed from bytes rather than through reqwest's `json` helper: the
        // client is built without that feature, because the extraction side
        // needs the payload's exact bytes and takes them the same way.
        let body = response
            .bytes()
            .await
            .map_err(|error| Self::unreachable(&error.to_string()))?;

        serde_json::from_slice::<T>(&body).map_err(|error| DeliveryError::Unidentifiable {
            destination: NAME.to_owned(),
            message: error.to_string(),
        })
    }

    /// # Errors
    ///
    /// [`DeliveryError::Unidentifiable`] if a request body will not serialise,
    /// which would be a defect in the rendering rather than anything the source
    /// did.
    fn encode<T: Serialize>(body: &T) -> Result<Vec<u8>, DeliveryError> {
        serde_json::to_vec(body).map_err(|error| DeliveryError::Unidentifiable {
            destination: NAME.to_owned(),
            message: error.to_string(),
        })
    }

    fn unreachable(message: &str) -> DeliveryError {
        DeliveryError::Unreachable {
            destination: NAME.to_owned(),
            message: message.to_owned(),
        }
    }
}

impl PrescriptionDestination for HevyRoutines {
    fn name(&self) -> &DestinationName {
        &self.name
    }

    async fn deliver(&self, session: &Deliverable) -> Result<Delivered, DeliveryError> {
        let folder = self.folder_for(session.programme.as_str()).await?;
        let rendered = render(session, folder);

        let response = self
            .client()?
            .post(self.url(ROUTINES_ENDPOINT))
            .header("api-key", &self.api_key)
            .header(CONTENT_TYPE, "application/json")
            .body(Self::encode(&CreateRoutine {
                routine: rendered.body,
            })?)
            .send()
            .await
            .map_err(|error| Self::unreachable(&error.to_string()))?;

        let created: CreatedRoutine = self.read(response).await?;

        // The source answers with a list, and has been seen to answer with one
        // element. Taking the first is not a guess: we sent one routine, so a
        // reply naming none is a reply we cannot record.
        let id = created
            .routine
            .into_iter()
            .next()
            .map(|routine| routine.id)
            .ok_or_else(|| DeliveryError::Unidentifiable {
                destination: NAME.to_owned(),
                message: "the reply named no routine".to_owned(),
            })?;

        let reference =
            DeliveryReference::try_from(id).map_err(|error| DeliveryError::Unidentifiable {
                destination: NAME.to_owned(),
                message: error.to_string(),
            })?;

        Ok(Delivered {
            reference,
            unexpressed: rendered.unexpressed,
        })
    }
}

/// The same rendering, stopped before it is sent.
///
/// **Not a mock.** It is the real [`render`] against the real prescription, and
/// what it hands back is the exact bytes [`HevyRoutines`] would post — which is
/// the only way to see, before anything is created that cannot be deleted, that
/// an assisted dip went out as assistance. The folder is unresolved because
/// resolving one would create it.
#[derive(Debug)]
pub struct HevyRoutinePreview {
    name: DestinationName,
    rendered: std::sync::Mutex<Option<String>>,
}

impl HevyRoutinePreview {
    /// # Errors
    ///
    /// [`DeliveryError::Unidentifiable`] if the compiled-in name is not a legal
    /// destination name. Pinned by a test.
    pub fn new() -> Result<Self, DeliveryError> {
        Ok(Self {
            name: DestinationName::try_from(NAME.to_owned()).map_err(|error| {
                DeliveryError::Unidentifiable {
                    destination: NAME.to_owned(),
                    message: error.to_string(),
                }
            })?,
            rendered: std::sync::Mutex::new(None),
        })
    }

    /// The body that would have been posted, if a session has been rendered.
    pub fn body(&self) -> Option<String> {
        self.rendered.lock().ok().and_then(|held| held.clone())
    }
}

impl PrescriptionDestination for HevyRoutinePreview {
    fn name(&self) -> &DestinationName {
        &self.name
    }

    async fn deliver(&self, session: &Deliverable) -> Result<Delivered, DeliveryError> {
        let rendered = render(session, None);
        let body = serde_json::to_string_pretty(&CreateRoutine {
            routine: rendered.body,
        })
        .map_err(|error| DeliveryError::Unidentifiable {
            destination: NAME.to_owned(),
            message: error.to_string(),
        })?;

        if let Ok(mut held) = self.rendered.lock() {
            *held = Some(body);
        }

        // A reference that could never be mistaken for one the source issued —
        // and one the caller is expected to throw away with the store it was
        // written to.
        let reference = DeliveryReference::try_from("preview".to_owned()).map_err(|error| {
            DeliveryError::Unidentifiable {
                destination: NAME.to_owned(),
                message: error.to_string(),
            }
        })?;

        Ok(Delivered {
            reference,
            unexpressed: rendered.unexpressed,
        })
    }
}

#[derive(Debug, Deserialize)]
struct FolderPage {
    #[serde(default)]
    routine_folders: Vec<Folder>,
}

#[derive(Debug, Deserialize)]
struct Folder {
    id: i64,
    title: String,
}

#[derive(Debug, Serialize)]
struct CreateFolder {
    routine_folder: FolderTitle,
}

#[derive(Debug, Serialize)]
struct FolderTitle {
    title: String,
}

#[derive(Debug, Deserialize)]
struct CreatedFolder {
    routine_folder: Folder,
}

/// **A list, because that is what the source sends back.** The create endpoint
/// answers with the routine wrapped in an array even though exactly one was
/// asked for, so this mirrors the wire rather than what would be tidier.
#[derive(Debug, Deserialize)]
struct CreatedRoutine {
    #[serde(default)]
    routine: Vec<CreatedRoutineBody>,
}

#[derive(Debug, Deserialize)]
struct CreatedRoutineBody {
    id: String,
}

#[cfg(test)]
mod tests {
    use super::{FOLDERS_ENDPOINT, HevyRoutines, NAME, ROUTINES_ENDPOINT};
    use application::DestinationName;

    /// **A stub cannot catch a wrong default.** The contract tests point this
    /// adapter at a local mock, so a base URL that already carried `/v1` would
    /// compose `/v1/v1/routines` and every one of them would still pass — which
    /// is exactly how `/v1/v1/workouts/events` reached a live run.
    #[test]
    fn the_base_url_and_the_endpoint_compose_to_the_real_url() {
        let hevy = HevyRoutines::new("https://api.hevyapp.com", "key")
            .expect("the compiled-in name is a legal destination name");

        assert_eq!(
            hevy.url(ROUTINES_ENDPOINT),
            "https://api.hevyapp.com/v1/routines"
        );
        assert_eq!(
            hevy.url(FOLDERS_ENDPOINT),
            "https://api.hevyapp.com/v1/routine_folders"
        );
    }

    #[test]
    fn a_trailing_slash_on_the_base_url_is_tolerated() {
        let hevy = HevyRoutines::new("https://api.hevyapp.com/", "key")
            .expect("the compiled-in name is a legal destination name");

        assert_eq!(
            hevy.url(ROUTINES_ENDPOINT),
            "https://api.hevyapp.com/v1/routines"
        );
    }

    /// The name is compiled in, so the fallible constructor's error arm is
    /// unreachable in practice rather than merely unlikely.
    #[test]
    fn the_compiled_in_name_is_a_legal_destination_name() {
        assert!(DestinationName::try_from(NAME.to_owned()).is_ok());
    }
}
