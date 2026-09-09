//! Hevy as a place a prescription can be put.
//!
//! **A renderer that returns a receipt.** Everything about *what* the session
//! instructs was settled before this adapter was called; what it adds is a
//! rendering ([`super::routine`]) and the identity the source gives what it is
//! handed.
//!
//! **And what the source said, whole** (#124). Until 2026-09-09 the identity was
//! the only thing that crossed back: the reply was read for an id and dropped,
//! which cost two diagnoses in a day — a create reply that would not parse, and
//! a routine that arrived missing an exercise the same reply would have shown.
//! The bytes now travel back over the port beside the outcome, taken before
//! anything interprets them, and the use case decides what to keep. Nothing here
//! writes to a store: a driven adapter reaching for another driven adapter is
//! the shape this avoids.
//!
//! ## Created, then updated in place
//!
//! **Decision 0022 revised this.** It used to say that nothing here calls `PUT`:
//! an issued prescription is written once, a reissue is a different
//! prescription, so every delivery was a `POST`. What that reasoning did not
//! survive was decision 0021 making re-derivation the ordinary case — a
//! corrected session then landed *beside* the one it corrected, and since the
//! source publishes no `DELETE`, the operator was left to tidy up by hand.
//!
//! So a routine id names a **place** rather than an issue: whichever
//! prescription for that date was last delivered into it. `deliver` creates,
//! `replace` updates, and a date has one routine however many times it is
//! corrected.
//!
//! What that costs is the property the old rule was protecting — a routine id no
//! longer names exactly one issued session, and the record shows why that
//! mattered: of the 8 landed workouts carrying a routine id, 5 carry the *same*
//! one, because that routine was rewritten in place. The answer is that the id
//! is no longer what pairs a performance with its prescription. The store hands
//! a place over rather than sharing it, so exactly one prescription holds a
//! reference at any time, and a workout naming it names that one.
//!
//! The source still publishes no `DELETE`, for a routine or for a folder, and
//! still retires the id of anything removed by hand. `replace` therefore fails
//! rather than falling back to a `POST` when the routine has gone: see
//! [`DeliveryError::Vanished`].
//!
//! ## The folder
//!
//! One per programme, resolved by title on the way past: found if it exists,
//! created if it does not. That is the only concession to the app's own shape in
//! this module, and it is a rendering decision — where a reader looks for the
//! session — rather than anything the domain knows about.

use std::{sync::OnceLock, time::Duration};

use application::{
    Deliverable, Delivered, DeliveryAttempt, DeliveryError, DeliveryReference, DestinationName,
    DestinationReply, PrescriptionDestination, ReplyStatus,
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

    /// Where one routine lives. The reference is the source's own id and goes
    /// back unaltered — it was opaque on the way in and is opaque on the way
    /// out.
    fn routine_endpoint(reference: &DeliveryReference) -> String {
        format!("{ROUTINES_ENDPOINT}/{reference}")
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
            let body: FolderPage = self
                .read(|client| {
                    client
                        .get(self.url(FOLDERS_ENDPOINT))
                        .header("api-key", &self.api_key)
                        .query(&[
                            ("page", page.to_string()),
                            ("pageSize", PAGE_SIZE.to_string()),
                        ])
                })
                .await?;
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
        let body = Self::encode(&CreateFolder {
            routine_folder: FolderTitle {
                title: title.to_owned(),
            },
        })?;

        let created: CreatedFolder = self
            .read(|client| {
                client
                    .post(self.url(FOLDERS_ENDPOINT))
                    .header("api-key", &self.api_key)
                    .header(CONTENT_TYPE, "application/json")
                    .body(body)
            })
            .await?;

        Ok(Some(created.routine_folder.id))
    }

    /// Build a request, send it, and take what came back.
    ///
    /// The three steps that fail before there is anything to keep, in one place
    /// so that the two acts below can say "either an answer or nothing" without
    /// spelling the alternatives out twice.
    async fn ask(
        &self,
        build: impl FnOnce(&Client) -> reqwest::RequestBuilder,
    ) -> Result<(StatusCode, DestinationReply), DeliveryError> {
        let response = build(self.client()?)
            .send()
            .await
            .map_err(|error| Self::unreachable(&error.to_string()))?;

        Self::answered(response).await
    }

    /// **Everything the destination said, before anything looks at it** (#124).
    ///
    /// The first thing done to a response and the only thing that consumes one,
    /// so there is no path through this adapter on which a reply is interpreted
    /// before it is taken. That ordering is the whole fix: the status used to
    /// decide whether the body was worth reading, which meant a refusal kept
    /// only a trimmed excerpt and an unparseable success kept nothing but
    /// serde's complaint.
    ///
    /// Taken as bytes rather than through reqwest's `json` helper — the client
    /// is built without that feature, exactly as on the extraction side, and for
    /// the same reason.
    ///
    /// # Errors
    ///
    /// [`DeliveryError::Unreachable`] if the body could not be read off the
    /// wire, which is a connection that failed part-way rather than an answer.
    ///
    /// The vendor's own [`StatusCode`] comes back beside the reply because two
    /// decisions here turn on it and neither belongs in the port's vocabulary: a
    /// refused credential and a routine that has gone. The reply carries only
    /// whether the act succeeded, which is the part that is not Hevy's to
    /// define.
    async fn answered(
        response: reqwest::Response,
    ) -> Result<(StatusCode, DestinationReply), DeliveryError> {
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|error| Self::unreachable(&error.to_string()))?;

        // `StatusCode` always displays as something, so the fallible
        // constructor's error arm is unreachable rather than merely unlikely —
        // and it is still handled, because § 26 does not make exceptions for
        // unlikely.
        let stated =
            ReplyStatus::new(status.to_string(), status.is_success()).map_err(|error| {
                DeliveryError::Unidentifiable {
                    destination: NAME.to_owned(),
                    message: error.to_string(),
                }
            })?;

        Ok((status, DestinationReply::new(stated, body.to_vec())))
    }

    /// Turn a refusal into the right error, having already kept what it said.
    ///
    /// **Unauthorised is `Unreachable`, not a panic and not a silent skip**: a
    /// credential that has been revoked degrades the system (§ 36) and leaves
    /// the prescription exactly where it was.
    ///
    /// **The body is deliberately not in the message.** It used to be, because
    /// the message was the only place it could be; now the reply travels back
    /// beside this error and the use case prints it there (#124), so repeating
    /// it here showed the operator the same bytes twice. [`Self::read`] is the
    /// exception and says why.
    ///
    /// Separate from [`Self::parse`] because an act whose answer carries nothing
    /// worth reading still has a status worth honouring.
    fn accepted(status: StatusCode) -> Result<(), DeliveryError> {
        if status.is_success() {
            return Ok(());
        }
        Err(Self::unreachable(&Self::glossed(status)))
    }

    /// A status, plus the one thing worth saying about it that it does not say
    /// itself.
    fn glossed(status: StatusCode) -> String {
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            format!("{status}: the API key was refused")
        } else {
            status.to_string()
        }
    }

    /// Read a reply as the shape it is supposed to be.
    fn parse<T: serde::de::DeserializeOwned>(reply: &DestinationReply) -> Result<T, DeliveryError> {
        serde_json::from_slice::<T>(reply.body()).map_err(|error| DeliveryError::Unidentifiable {
            destination: NAME.to_owned(),
            message: error.to_string(),
        })
    }

    /// Send it, keep the answer, and read the answer as `T`.
    ///
    /// The folder calls' whole path. Their replies are not recorded: what #124
    /// is about is the answer to the *delivery*, and a folder lookup on the way
    /// past is not that. A folder call that goes wrong still fails the delivery,
    /// and still says what it said in the error it raises.
    async fn read<T: serde::de::DeserializeOwned>(
        &self,
        build: impl FnOnce(&Client) -> reqwest::RequestBuilder,
    ) -> Result<T, DeliveryError> {
        let (status, reply) = self.ask(build).await?;

        // **The one refusal that carries its own body.** A folder call's reply
        // is not the delivery's answer and so is not kept — what it said is in
        // this message or it is nowhere.
        if !status.is_success() {
            let gloss = Self::glossed(status);
            let said = reply.text();
            let said = said.trim();
            return Err(Self::unreachable(&if said.is_empty() {
                gloss
            } else {
                format!("{gloss}: {said}")
            }));
        }

        Self::parse(&reply)
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

    async fn deliver(&self, session: &Deliverable) -> DeliveryAttempt {
        let folder = match self.folder_for(session.plan.as_str()).await {
            Ok(folder) => folder,
            // Nothing has been sent to `/v1/routines` yet, so there is no reply
            // to this delivery — whatever the folder call said, it answered a
            // different question.
            Err(error) => return DeliveryAttempt::unanswered(error),
        };
        let rendered = render(session, folder);

        let body = match Self::encode(&CreateRoutine {
            routine: rendered.body,
        }) {
            Ok(body) => body,
            Err(error) => return DeliveryAttempt::unanswered(error),
        };

        let answered = self
            .ask(|client| {
                client
                    .post(self.url(ROUTINES_ENDPOINT))
                    .header("api-key", &self.api_key)
                    .header(CONTENT_TYPE, "application/json")
                    .body(body)
            })
            .await;

        let (status, reply) = match answered {
            Ok(answered) => answered,
            Err(error) => return DeliveryAttempt::unanswered(error),
        };

        // **Everything below interprets a reply that is already in hand**, which
        // is what #124 is about: the id, the status and the parse all happen
        // after the bytes have been taken, so the use case records what Hevy
        // said whether or not any of the three works out.
        let outcome = Self::accepted(status)
            .and_then(|()| Self::parse::<CreatedRoutine>(&reply))
            // **The only arm that has to read the reply.** The routine exists by
            // the time this runs and its id is nowhere else, so a reply this
            // cannot parse is a session on the operator's phone that the store
            // does not know about — which is why the shape read is the observed
            // one rather than the documented one.
            .and_then(|created| {
                DeliveryReference::try_from(created.routine.id).map_err(|error| {
                    DeliveryError::Unidentifiable {
                        destination: NAME.to_owned(),
                        message: error.to_string(),
                    }
                })
            })
            .map(|reference| Delivered {
                reference,
                unexpressed: rendered.unexpressed,
            });

        DeliveryAttempt::answered(reply, outcome)
    }

    async fn replace(
        &self,
        session: &Deliverable,
        occupying: &DeliveryReference,
    ) -> DeliveryAttempt {
        let folder = match self.folder_for(session.plan.as_str()).await {
            Ok(folder) => folder,
            Err(error) => return DeliveryAttempt::unanswered(error),
        };
        let rendered = render(session, folder);

        // **The same body as `deliver` sends.** `PutRoutinesRequestBody` and
        // `PostRoutinesRequestBody` are the same object field for field, down to
        // the exercise and set schemas, so the rendering is shared rather than
        // mirrored — a second renderer would be two places for one decision
        // about what a session looks like in the app.
        let body = match Self::encode(&CreateRoutine {
            routine: rendered.body,
        }) {
            Ok(body) => body,
            Err(error) => return DeliveryAttempt::unanswered(error),
        };

        let answered = self
            .ask(|client| {
                client
                    .put(self.url(&Self::routine_endpoint(occupying)))
                    .header("api-key", &self.api_key)
                    .header(CONTENT_TYPE, "application/json")
                    .body(body)
            })
            .await;

        let (status, reply) = match answered {
            Ok(answered) => answered,
            Err(error) => return DeliveryAttempt::unanswered(error),
        };

        // A routine that is not there is its own answer rather than a transport
        // failure: the store believes the operator has this session and the app
        // says otherwise, and only they can say which is right. The reply comes
        // back with it, so what Hevy said about the routine it does not have is
        // kept rather than inferred.
        let outcome = if status == StatusCode::NOT_FOUND {
            Err(DeliveryError::Vanished {
                destination: NAME.to_owned(),
                reference: occupying.to_string(),
                date: session.workout.issued_for(),
            })
        } else {
            // **And the reply's shape is not read at all, because the reference
            // is already known.** A `PUT` answers about the routine its path
            // names, so an id in the body could only be the one that was sent —
            // and reading it would put the handover at the mercy of a shape
            // nobody has confirmed live. The create endpoint's shape moved under
            // us once (#119), after the routine existed; this arm cannot pay
            // that price because it asks the body for nothing. Its status still
            // has to be right, and the body is still kept: not reading a reply
            // is not a reason to discard it.
            Self::accepted(status).map(|()| Delivered {
                reference: occupying.clone(),
                unexpressed: rendered.unexpressed,
            })
        };

        DeliveryAttempt::answered(reply, outcome)
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

    /// **Answers nothing, because it asked nothing.** A preview contacts no
    /// destination, so there is no reply to keep — and inventing one would put a
    /// row in the store claiming Hevy said something it was never asked.
    async fn deliver(&self, session: &Deliverable) -> DeliveryAttempt {
        let rendered = render(session, None);
        let body = match serde_json::to_string_pretty(&CreateRoutine {
            routine: rendered.body,
        }) {
            Ok(body) => body,
            Err(error) => {
                return DeliveryAttempt::unanswered(DeliveryError::Unidentifiable {
                    destination: NAME.to_owned(),
                    message: error.to_string(),
                });
            }
        };

        if let Ok(mut held) = self.rendered.lock() {
            *held = Some(body);
        }

        // A reference that could never be mistaken for one the source issued —
        // and one the caller is expected to throw away with the store it was
        // written to.
        let outcome = DeliveryReference::try_from("preview".to_owned())
            .map_err(|error| DeliveryError::Unidentifiable {
                destination: NAME.to_owned(),
                message: error.to_string(),
            })
            .map(|reference| Delivered {
                reference,
                unexpressed: rendered.unexpressed,
            });

        DeliveryAttempt {
            reply: None,
            outcome,
        }
    }

    /// **A preview renders the same body whichever act it stands in for.** What
    /// the operator is checking is what the session says, and `PUT` and `POST`
    /// send byte-identical bodies — so replacing shows exactly what replacing
    /// would send, and the reference it hands back is the one it was aimed at
    /// rather than a fresh invention.
    async fn replace(
        &self,
        session: &Deliverable,
        occupying: &DeliveryReference,
    ) -> DeliveryAttempt {
        let attempt = self.deliver(session).await;
        DeliveryAttempt {
            reply: attempt.reply,
            outcome: attempt.outcome.map(|delivered| Delivered {
                reference: occupying.clone(),
                unexpressed: delivered.unexpressed,
            }),
        }
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

/// **One routine under a wrapper, because that is what the source sends back.**
/// This mirrors the wire rather than what would be tidier — and the wire moved:
/// `POST /v1/routines` answered `{"routine": [{…}]}` until 2026-09-09, when a
/// live run met `{"routine": {…}}` and could not read the id of a routine the
/// app had already created (#119).
///
/// Mirroring is still the rule, and the published spec is not the arbiter: it
/// describes this reply as a bare routine under no key at all, which is a third
/// shape and not the one that arrived. What is here is what was observed.
#[derive(Debug, Deserialize)]
struct CreatedRoutine {
    routine: CreatedRoutineBody,
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
