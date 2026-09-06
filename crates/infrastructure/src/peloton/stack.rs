//! The rider's stack: what Peloton will play next.
//!
//! **One queue, not a place per date.** Hevy holds a routine for each date, so
//! delivering twice replaces that date's routine and touches nothing else. The
//! Peloton stack is a single ordered queue for the rider, shared with every
//! discipline and every device, and it can only be replaced whole — so
//! delivering into it can discard something this tool never put there. That is
//! the one asymmetry between the two destinations, and it is why the command
//! asks before replacing.
//!
//! **Only `addClassToStack` writes.** `modifyStack` takes a whole list, answers
//! success, and changes nothing — whatever identifier it is given. It clears,
//! and that is all it does, so setting the stack is a clear followed by one add
//! per class. Believing its 200 cost this adapter an afternoon, twice.
//!
//! **A class is named by a join token, not by its id** (#70). A raw ride id
//! decodes to nothing and the resolver reports a lookup miss as an empty stack
//! rather than an error, so a wrong identifier answers 200 and changes nothing —
//! which is how the stack was believed to be write-only for a fortnight. The
//! token is constructed here and appears nowhere above this module.
//!
//! **It is a different endpoint from everything else in this adapter.** Classes
//! are read over REST from `api.onepeloton.com`; the stack is GraphQL at
//! `gql-graphql-gateway.prod.k8s.onepeloton.com`, and `api.onepeloton.com/graphql`
//! answers 404.

use application::SourceError;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;

use super::auth::PelotonAuth;

/// Peloton's GraphQL gateway.
///
/// Its own constant rather than the API base: they are different services, and
/// a stack pointed at the REST host fails with a 404 that looks like an outage.
pub const GATEWAY: &str = "https://gql-graphql-gateway.prod.k8s.onepeloton.com/graphql";

/// What a class is called when it is stacked.
///
/// **Base64 of a small JSON object**, which is Peloton's own encoding rather
/// than ours: the field names are theirs, and `on_demand` is the only `type`
/// this tool has cause to send — a live class is not something a programme can
/// place, since it has already happened or has not happened yet.
///
/// Written by hand rather than through `serde` because the shape is fixed, has
/// three constants in it, and a struct would invite someone to add a field the
/// far side does not read.
#[must_use]
pub fn join_token(ride_id: &str) -> String {
    let json = format!(
        r#"{{"home_peloton_id": null, "ride_id": "{ride_id}", "studio_peloton_id": null, "type": "on_demand"}}"#
    );
    STANDARD.encode(json)
}

/// What the stack holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stacked {
    /// The join tokens in it, in the order they will play.
    pub classes: Vec<String>,
    /// How long the whole stack runs, in seconds, as Peloton counts it.
    pub total_seconds: u64,
}

impl Stacked {
    #[must_use]
    pub const fn count(&self) -> usize {
        self.classes.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }
}

/// The rider's stack, at Peloton.
#[derive(Debug)]
pub struct PelotonStack {
    gateway: String,
    auth: PelotonAuth,
    client: std::sync::OnceLock<Result<reqwest::Client, String>>,
}

impl PelotonStack {
    /// `gateway` is the GraphQL endpoint — [`GATEWAY`] in production, a stub in
    /// the contract tests.
    ///
    /// **Constructing this does no I/O**, the rule every adapter here follows.
    pub const fn new(gateway: String, auth: PelotonAuth) -> Self {
        Self {
            gateway,
            auth,
            client: std::sync::OnceLock::new(),
        }
    }

    /// What is in the stack now.
    ///
    /// **The list, not the counters.** `numClasses` and `totalTime` read zero
    /// both for an empty stack and for a write that missed, so anything deciding
    /// whether a write landed has to read `stackedClassList` — which is the
    /// mistake #70 was open on for a fortnight.
    ///
    /// # Errors
    ///
    /// [`SourceError`] if the gateway is unreachable, refuses the token, or
    /// answers something this cannot read.
    pub async fn view(&self) -> Result<Stacked, SourceError> {
        let answer: ViewAnswer = self
            .ask(
                "query { viewUserStack { totalTime userStack \
                 { stackedClassList { pelotonClassId } } } }",
            )
            .await?;
        Ok(Stacked {
            classes: answer
                .view_user_stack
                .user_stack
                .stacked_class_list
                .into_iter()
                .map(|class| class.peloton_class_id)
                .collect(),
            total_seconds: answer.view_user_stack.total_time,
        })
    }

    /// Replace the stack with these classes, in this order.
    ///
    /// **Two mutations, because only one of them works.** `modifyStack` takes a
    /// whole list and is a silent no-op for setting one — join token or raw id,
    /// it answers success and changes nothing, which is the same failure #70 was
    /// opened on and which cost this adapter a second afternoon. What it *does*
    /// do is clear, so the sequence is: empty the stack with `modifyStack`, then
    /// add each class with `addClassToStack`, which works.
    ///
    /// Adding one at a time is also the only ordering available: there is no
    /// `removeClassFromStack`, so the order they go in is the order they play.
    ///
    /// **The result is checked against what was asked for.** Twice now a write
    /// has been believed on the strength of a 200, and twice it had done
    /// nothing — so this reads the list back and refuses rather than reporting a
    /// success it has not seen.
    ///
    /// # Errors
    ///
    /// [`SourceError`] as [`view`](Self::view) gives it, and
    /// [`SourceError::Malformed`] if the stack does not end up holding what was
    /// sent.
    pub async fn set(&self, ride_ids: &[String]) -> Result<Stacked, SourceError> {
        self.clear().await?;
        for ride in ride_ids {
            self.add(ride).await?;
        }

        let stacked = self.view().await?;
        let wanted: Vec<String> = ride_ids.iter().map(|id| join_token(id)).collect();
        if stacked.classes != wanted {
            return Err(SourceError::Malformed {
                detail: format!(
                    "the stack was asked for {} class(es) and holds {} — Peloton reported \
                     success and did something else",
                    wanted.len(),
                    stacked.count(),
                ),
            });
        }
        Ok(stacked)
    }

    /// Empty the stack.
    ///
    /// **The one thing `modifyStack` is good for.** There is no `clearStack` and
    /// no `removeClassFromStack`, so an empty list is the only way to take
    /// anything out.
    ///
    /// # Errors
    ///
    /// [`SourceError`] as [`view`](Self::view) gives it.
    pub async fn clear(&self) -> Result<(), SourceError> {
        let _: ModifyAnswer = self
            .ask(
                "mutation { modifyStack(input: {pelotonClassIdList: []}) \
                 { totalTime userStack { stackedClassList { pelotonClassId } } } }",
            )
            .await?;
        Ok(())
    }

    /// Put one class on the end of the stack.
    ///
    /// # Errors
    ///
    /// [`SourceError`] as [`view`](Self::view) gives it.
    pub async fn add(&self, ride_id: &str) -> Result<Stacked, SourceError> {
        let document = format!(
            "mutation {{ addClassToStack(input: {{pelotonClassId: \"{}\"}}) \
             {{ totalTime userStack {{ stackedClassList {{ pelotonClassId }} }} }} }}",
            join_token(ride_id)
        );
        let answer: AddAnswer = self.ask(&document).await?;
        Ok(Stacked {
            classes: answer
                .add_class_to_stack
                .user_stack
                .stacked_class_list
                .into_iter()
                .map(|class| class.peloton_class_id)
                .collect(),
            total_seconds: answer.add_class_to_stack.total_time,
        })
    }

    /// Send one document and read the `data` out of the answer.
    async fn ask<T: serde::de::DeserializeOwned>(&self, document: &str) -> Result<T, SourceError> {
        let bearer = self.auth.bearer().await?;
        let response = self
            .client()?
            .post(&self.gateway)
            .bearer_auth(bearer)
            .header("Peloton-Platform", "web")
            .json(&serde_json::json!({ "query": document }))
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SourceError::Unauthorised);
        }
        let body = response
            .text()
            .await
            .map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
            })?;

        // **GraphQL answers 200 with an `errors` array**, and a 400 carries one
        // too. Reading the status alone would take "cannot query field" for
        // success, which is the whole family of faults this adapter has already
        // been caught by once.
        let envelope: Envelope<T> =
            serde_json::from_str(&body).map_err(|error| SourceError::Malformed {
                detail: format!("the gateway answered {status} with something unreadable: {error}"),
            })?;
        if let Some(errors) = envelope.errors.filter(|errors| !errors.is_empty()) {
            let said: Vec<&str> = errors.iter().map(|error| error.message.as_str()).collect();
            return Err(SourceError::Malformed {
                detail: format!("the gateway refused the request: {}", said.join("; ")),
            });
        }
        envelope.data.ok_or_else(|| SourceError::Malformed {
            detail: format!("the gateway answered {status} with neither data nor errors"),
        })
    }

    fn client(&self) -> Result<&reqwest::Client, SourceError> {
        self.client
            .get_or_init(|| {
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(|detail| SourceError::Unavailable {
                detail: detail.clone(),
            })
    }
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQlError>>,
}

#[derive(Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewAnswer {
    view_user_stack: StackAnswer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModifyAnswer {
    #[expect(dead_code, reason = "clearing reads nothing back; view does that")]
    modify_stack: StackAnswer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddAnswer {
    add_class_to_stack: StackAnswer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StackAnswer {
    total_time: u64,
    user_stack: UserStack,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserStack {
    stacked_class_list: Vec<StackedClass>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StackedClass {
    peloton_class_id: String,
}
