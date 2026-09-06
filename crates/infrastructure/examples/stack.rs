//! Probe Peloton's Stack and Schedule APIs — issue #70.
//!
//! **What #70 found**: `modifyStack` and `addClassToStack` accept a well-formed
//! call, answer 200 with a success payload, and change nothing. **Confirmed here
//! on 2026-09-06, and against a stronger reading**: that verdict rested on
//! `numClasses` and `totalTime` both being zero, which two counters could be
//! zero for other reasons. `userStack.stackedClassList` is the actual list, and
//! it is `[]`.
//!
//! **What #70 did not know**: there is a second write surface, and it is not the
//! stack.
//!
//! ```text
//! mutation addClassToSchedule(scheduledClassInput: ScheduledClassInput!): ScheduledClassResponse!
//! mutation rescheduleClass(rescheduleClassInput: RescheduleClassInput!):   ScheduledClassResponse!
//! mutation removeClassFromSchedule(scheduledClassId: String!):             ScheduledClassResponse!
//! query    scheduledClass(scheduledClassId: String!):                      ScheduledClassResponse!
//! query    userScheduledItemsList(startTime: DateTime!, endTime: DateTime!): YourScheduleListResponse!
//!
//! input ScheduledClassInput { id: ID!, scheduledStartTime: … }
//! union ScheduledClassResponse   -- needs an inline fragment on ScheduledClass
//! ```
//!
//! That is the operator's calendar rather than his stack, and it is what
//! decision 0025 actually asked for: *a session should ideally be scheduled into
//! the operator's Peloton calendar*. **Whether it writes is untested** — it has
//! not been called, because calling it puts a real entry in a real calendar.
//!
//! ## How the schema was read, since introspection is off
//!
//! The gateway is `gql-graphql-gateway.prod.k8s.onepeloton.com/graphql`;
//! `api.onepeloton.com/graphql` answers 404 and `graph.onepeloton.com` is not a
//! server. It is an Apollo server with `introspection: false`, so `__schema` is
//! refused — but it still answers a wrong field with *"Cannot query field X on
//! type Y. Did you mean Z?"*, and the suggestions come from the real schema. So
//! the schema is read a few names at a time by guessing near-misses, which is
//! what [`GUESSES`] is for. Edit it and re-run.
//!
//! **Every guess below is a read.** Nothing here writes, and nothing here should
//! be made to write without the operator saying so first.
//!
//! ```text
//! set -a; . ./.env; set +a
//! cargo run -p infrastructure --example stack
//! ```

use std::fmt::Write as _;

use infrastructure::peloton::auth::{PelotonAuth, PelotonCredentials};

const AUTH_BASE: &str = "https://auth.onepeloton.com";

/// The gateway. `api.onepeloton.com/graphql` answers 404 and
/// `graph.onepeloton.com` does not resolve to a server; this is the Apollo
/// server that actually parses a GraphQL document.
const GATEWAY: &str = "https://gql-graphql-gateway.prod.k8s.onepeloton.com/graphql";

/// The documents to send, as `(operation, selection)`.
///
/// A correct one returns data; a near-miss returns Apollo's suggestions, which
/// is how the schema above was read. Both are useful and neither writes.
const GUESSES: [(&str, &str); 3] = [
    // The stack, read properly: the list rather than the two counters.
    (
        "query",
        "viewUserStack { numClasses totalTime userStack { stackedClassList { pelotonClassId } } }",
    ),
    // The schedule. A near-miss on purpose, to keep printing the input shape
    // without calling the mutation.
    (
        "mutation",
        "addClassToSchedule(scheduledClassInput: {id: \"probe\", probe: 1}) { probe }",
    ),
    (
        "query",
        "userScheduledItemsList(startTime: \"2026-09-01T00:00:00Z\", endTime: \"2026-12-31T00:00:00Z\") { probe }",
    ),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let email = std::env::var("PELOTON_EMAIL")?;
    let password = std::env::var("PELOTON_PASSWORD")?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let auth = PelotonAuth::new(AUTH_BASE, PelotonCredentials::new(email, password));
        let bearer = auth.bearer().await?;
        let client = reqwest::Client::builder()
            .user_agent("fitness-tracker/0.2 (+stack probe, issue #70)")
            .build()?;

        for (operation, field) in GUESSES {
            let document = format!("{operation} {{ {field} }}");
            println!("\n=== {document}");
            match ask(&client, &bearer, &document).await {
                Ok(answer) => println!("{answer}"),
                Err(error) => println!("  {error}"),
            }
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

/// Send one document and report what came back, messages first.
async fn ask(
    client: &reqwest::Client,
    bearer: &str,
    document: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let response = client
        .post(GATEWAY)
        .bearer_auth(bearer)
        .header("Peloton-Platform", "web")
        .json(&serde_json::json!({ "query": document }))
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;
    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(format!("  {status}: {}", first_line(&body))),
    };

    let mut report = String::new();
    writeln!(report, "  {status}")?;
    if let Some(errors) = parsed.get("errors").and_then(serde_json::Value::as_array) {
        for error in errors {
            if let Some(message) = error.get("message").and_then(serde_json::Value::as_str) {
                writeln!(report, "  {}", wrap(message))?;
            }
        }
    }
    if let Some(data) = parsed.get("data").filter(|data| !data.is_null()) {
        writeln!(report, "  data: {}", first_line(&data.to_string()))?;
    }
    Ok(report)
}

/// Apollo's suggestion lists run long; break them so the report stays readable.
fn wrap(message: &str) -> String {
    message.replace("Did you mean", "\n    Did you mean")
}

fn first_line(body: &str) -> String {
    body.lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(200)
        .collect()
}
