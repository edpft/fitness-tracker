//! Probe Peloton's Stack API — issue #70.
//!
//! **The stack accepts writes.** #70 concluded it did not: `addClassToStack`
//! answered 200 with `numClasses: 0` and the stack stayed empty. The mutation
//! was fine and the identifier was wrong.
//!
//! **`pelotonClassId` is a join token, not a ride id** — base64 of
//! `{"home_peloton_id": null, "ride_id": <id>, "studio_peloton_id": null,
//! "type": "on_demand"}`. A raw ride id decodes to nothing, which is why it
//! looked exactly as bogus as the string `not-a-class-id-at-all`: both were
//! lookup misses, and the resolver reports a miss as an empty stack rather than
//! an error. Found in `jasonmotylinski/peloton-planner`, which the operator
//! pointed at on 2026-09-06; proven here the same day, `numClasses: 1,
//! totalTime: 2700` for a 45-minute class, then cleared.
//!
//! ```text
//! mutation addClassToStack(input: AddClassToStackInput!): StackResponse!   -- pelotonClassId: ID!
//! mutation modifyStack(input: ModifyStackInput!):         StackResponse!   -- pelotonClassIdList: [ID!]!  (replaces; [] clears)
//! mutation playClassFromStack(input: PlayClassFromStackInput!)
//! query    viewUserStack:                                 StackResponse!
//!
//! StackResponse { numClasses, totalTime, userStack { stackedClassList { pelotonClassId } } }
//! ```
//!
//! **Read the list, not the counters.** `numClasses` and `totalTime` are zero
//! both when the stack is empty and when a write missed, so a test that watches
//! only those cannot tell success from silence — which is how #70 reached the
//! wrong verdict. `userStack.stackedClassList` is what a write has to change.
//!
//! **The schedule is the wrong shape and is not used.** `addClassToSchedule`
//! exists and takes a `scheduledStartTime`; the operator, 2026-09-06: *"the
//! problem with scheduling as opposed to adding classes to a stack is that it
//! assumes a time, and that time needs to be flexible."* A prescription says
//! Wednesday, not half past six.
//!
//! ## Reading a schema with introspection disabled
//!
//! The gateway is `gql-graphql-gateway.prod.k8s.onepeloton.com/graphql`;
//! `api.onepeloton.com/graphql` answers 404. It is an Apollo server with
//! `introspection: false`, so `__schema` is refused — but a wrong field still
//! comes back as *"Cannot query field X on type Y. Did you mean Z?"*, and the
//! suggestions are drawn from the real schema. That is what [`GUESSES`] is for:
//! put a near-miss in it and re-run.
//!
//! ```text
//! set -a; . ./.env; set +a
//! cargo run -p infrastructure --example stack
//! ```
//!
//! **This writes to a real account.** What is in `GUESSES` now is a read.

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
const PLATFORMS: [Option<&str>; 1] = [Some("web")];

const GUESSES: [(&str, &str); 2] = [
    (
        "mutation",
        "modifyStack(input: {pelotonClassIdList: []}) { numClasses totalTime }",
    ),
    (
        "query",
        "viewUserStack { numClasses totalTime userStack { stackedClassList { pelotonClassId } } }",
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

        for platform in PLATFORMS {
            for (operation, field) in GUESSES {
                let document = format!("{operation} {{ {field} }}");
                println!("\n=== [{}] {document}", platform.unwrap_or("no header"));
                match ask(&client, &bearer, &document, platform).await {
                    Ok(answer) => println!("{answer}"),
                    Err(error) => println!("  {error}"),
                }
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
    platform: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut request = client
        .post(GATEWAY)
        .bearer_auth(bearer)
        .json(&serde_json::json!({ "query": document }));
    if let Some(platform) = platform {
        request = request.header("Peloton-Platform", platform);
    }
    let response = request.send().await?;

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
