//! The pages, driven in headless Chromium as htmx drives them (#830).
//!
//! **A browser, not a request**: on 2026-11-17 every card waiting on a run
//! stayed a placeholder on the operator's phone, because each event naming a
//! part collected was sent with no data and a browser drops such an event
//! unseen. `curl` showed every one of them.
//!
//! `CHROMIUM` names the browser. The dev shell and the gate set it, and the
//! test fails rather than skips where it is not set.

use std::{collections::BTreeSet, path::Path, process::Stdio, sync::Arc, time::Duration};

use application::Part;
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt as _, BufReader},
    net::TcpStream,
    process::{Child, Command},
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

use crate::run::{Asked, Runs, Setting};

/// A headless Chromium, and the one page it drives.
struct Browser {
    _process: Child,
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    session: String,
    sent: u64,
    /// Whether a page has been loaded, so the next is reached by a link.
    loaded: bool,
}

/// Set on a page as its connection to the run opens, so nothing is sent to it
/// before it can hear.
///
/// **What htmx did to each card is kept**, so a card left asking says why.
const LISTENING: &str = "window.happened = []; window.started = Date.now();
document.addEventListener('htmx:sseOpen', (event) => {
    if (event.target.classList && event.target.classList.contains('connected')) {
        window.listening = true;
    }
});
for (const name of ['htmx:sseMessage', 'htmx:beforeRequest', 'htmx:afterRequest',
                    'htmx:sendError', 'htmx:responseError', 'htmx:swapError', 'htmx:sseError']) {
    document.addEventListener(name, (event) => {
        const card = event.target && event.target.getAttribute && event.target.getAttribute('data-card');
        const status = event.detail && event.detail.xhr ? ' ' + event.detail.xhr.status : '';
        const type = event.detail && event.detail.type ? ' ' + event.detail.type : '';
        window.happened.push((Date.now() - window.started) + ' ' + name + ' ' + (card || event.target.tagName) + type + status);
    }, true);
}";

/// The cards still asking to be drawn: a placeholder, or a drawing waiting on
/// a trigger.
const ASKING: &str = "Array.from(document.querySelectorAll('[data-card][hx-trigger]'))
    .map((card) => card.getAttribute('data-card'))";

impl Browser {
    /// Chromium at `chromium`, its profile in `profile`, on a blank page.
    async fn launch(chromium: &str, profile: &Path) -> Result<Self, String> {
        let mut process = Command::new(chromium)
            .args([
                "--headless=new",
                "--no-sandbox",
                "--disable-gpu",
                "--disable-dev-shm-usage",
                "--no-first-run",
                "--remote-debugging-port=0",
            ])
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg("about:blank")
            .env("HOME", profile)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("{chromium} could not be started: {error}"))?;
        let stderr = process
            .stderr
            .take()
            .ok_or("Chromium's stderr was not piped")?;
        let mut lines = BufReader::new(stderr).lines();
        let address = tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(line) = lines.next_line().await.map_err(|error| error.to_string())? {
                if let Some(address) = line.strip_prefix("DevTools listening on ") {
                    return Ok(address.to_owned());
                }
            }
            Err("Chromium ended without listening".to_owned())
        })
        .await
        .map_err(|_| "Chromium did not listen within 30 s".to_owned())??;
        // Its later complaints are read and let go, so a full pipe never
        // stops it.
        tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
        let (socket, _) = tokio_tungstenite::connect_async(address.as_str())
            .await
            .map_err(|error| error.to_string())?;
        let mut browser = Self {
            _process: process,
            socket,
            session: String::new(),
            sent: 0,
            loaded: false,
        };
        let target = browser
            .command("Target.createTarget", json!({ "url": "about:blank" }))
            .await?;
        let attached = browser
            .command(
                "Target.attachToTarget",
                json!({ "targetId": target["targetId"], "flatten": true }),
            )
            .await?;
        browser.session = attached["sessionId"]
            .as_str()
            .ok_or("no session")?
            .to_owned();
        browser.command("Page.enable", json!({})).await?;
        browser.command("Runtime.enable", json!({})).await?;
        browser
            .command(
                "Page.addScriptToEvaluateOnNewDocument",
                json!({ "source": LISTENING }),
            )
            .await?;
        Ok(browser)
    }

    /// Send `method` and wait for its answer, letting every event go.
    async fn command(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.sent += 1;
        let id = self.sent;
        let mut message = json!({ "id": id, "method": method, "params": params });
        if !self.session.is_empty() {
            message["sessionId"] = json!(self.session);
        }
        self.socket
            .send(Message::text(message.to_string()))
            .await
            .map_err(|error| error.to_string())?;
        while let Some(received) = self.socket.next().await {
            let Message::Text(text) = received.map_err(|error| error.to_string())? else {
                continue;
            };
            let answer: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
            if answer["id"] == json!(id) {
                if let Some(error) = answer.get("error") {
                    return Err(format!("{method}: {error}"));
                }
                return Ok(answer["result"].clone());
            }
        }
        Err(format!("{method}: the connection closed"))
    }

    /// What `expression` evaluates to on the page.
    async fn evaluate(&mut self, expression: &str) -> Result<Value, String> {
        let evaluated = self
            .command(
                "Runtime.evaluate",
                json!({ "expression": expression, "returnByValue": true }),
            )
            .await?;
        if let Some(thrown) = evaluated.get("exceptionDetails") {
            return Err(format!("{expression}: {thrown}"));
        }
        Ok(evaluated["result"]["value"].clone())
    }

    /// Open `url` and wait until its connection to the run is open: loaded,
    /// the first time, and from then on **reached by a link**, as the
    /// operator reaches one, which htmx follows by swapping the page in.
    async fn open(&mut self, url: &str) -> Result<(), String> {
        if self.loaded {
            let followed = format!(
                "{{
                    window.listening = false;
                    window.happened = [];
                    const link = document.createElement('a');
                    link.href = {};
                    document.querySelector('main').appendChild(link);
                    htmx.process(link);
                    link.click();
                }}",
                json!(url)
            );
            self.evaluate(&followed).await?;
        } else {
            self.command("Page.navigate", json!({ "url": url })).await?;
            self.loaded = true;
        }
        self.until("window.listening === true", |listening| {
            *listening == json!(true)
        })
        .await
        .map(|_| ())
        .map_err(|last| format!("{url} never connected: {last}"))
    }

    /// `expression`, polled until `done` says it is, for at most five seconds:
    /// its last value either way.
    async fn until(
        &mut self,
        expression: &str,
        done: impl Fn(&Value) -> bool,
    ) -> Result<Value, Value> {
        let mut last = Value::Null;
        for _ in 0..50 {
            last = self.evaluate(expression).await.map_err(Value::String)?;
            if done(&last) {
                return Ok(last);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(last)
    }
}

/// How the run a page opened during ends.
#[derive(Debug, Clone, Copy)]
enum Ending {
    /// Every part it reads is collected, then the run ends.
    Collected,
    /// The run ends with nothing collected, as one that fails does.
    Ended,
}

/// What `page` showed, opened while a run over everything went and ended as
/// `ending` says: the cards that waited on it, and those still asking once it
/// had ended.
async fn opened(
    browser: &mut Browser,
    runs: &Runs,
    url: &str,
    parts: &[Part],
    ending: Ending,
) -> Result<(Value, Value), String> {
    runs.going_over(&Asked::Everything);
    browser.open(url).await?;
    let waited = browser.evaluate(ASKING).await?;
    if matches!(ending, Ending::Collected) {
        runs.collect(parts);
    }
    runs.ended_over(&Asked::Everything);
    match browser
        .until(ASKING, |asking| {
            asking.as_array().is_some_and(Vec::is_empty)
        })
        .await
    {
        Ok(asking) => Ok((waited, asking)),
        Err(asking) => Ok((
            waited,
            json!({ "asking": asking, "happened": browser.evaluate("window.happened").await? }),
        )),
    }
}

/// **Every card on every page is drawn** (#830): each page is opened in a
/// browser while a run over everything goes, and once it ends no card is
/// left asking to be drawn: neither when each part it read was collected,
/// nor when the run ended without them.
#[test]
fn every_card_on_every_page_is_drawn() {
    let chromium = std::env::var("CHROMIUM")
        .expect("CHROMIUM names a headless Chromium: the dev shell and the gate set it");
    let directory = tempfile::TempDir::new().expect("a temporary directory");
    // **Several threads, as `serve` has**: a card's drawing waits on work
    // the server's other threads do.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let runs = Arc::new(Runs::new(Setting {
        store: runtime
            .block_on(configurator::store::Store::open(
                &directory.path().join("fitness.db"),
            ))
            .expect("the store"),
        zone: domain::normalised::OperatorZone::try_from("Europe/London").expect("a zone"),
        credentials: None,
        build: Some("a build"),
    }));
    let pages = crate::pages();
    let routes = pages.carded().to_vec();
    let parts: Vec<Part> = pages
        .reads()
        .iter()
        .flat_map(|(_, reads)| {
            crate::card::scope(reads)
                .parts()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
        })
        .chain([Part::Delivering])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let (waited, asking) = runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a port");
        let address = listener.local_addr().expect("an address");
        let served = Arc::clone(&runs);
        tokio::spawn(async move { axum::serve(listener, crate::app(served)).await });
        let mut browser = Browser::launch(&chromium, &directory.path().join("chromium"))
            .await
            .expect("Chromium, driven");
        let mut waited = Vec::new();
        let mut asking = Vec::new();
        for ending in [Ending::Collected, Ending::Ended] {
            for route in &routes {
                let (waits, asks) = opened(
                    &mut browser,
                    &runs,
                    &format!("http://{address}{route}"),
                    &parts,
                    ending,
                )
                .await
                .unwrap_or_else(|error| panic!("{route}, {ending:?}: {error}"));
                waited.push(format!("{route} {ending:?}: {waits}"));
                if asks != json!([]) {
                    asking.push(format!("{route} {ending:?}: {asks}"));
                }
            }
        }
        (waited, asking)
    });

    assert!(
        waited.iter().all(|line| !line.ends_with("[]")),
        "every page has a card that waits on the run: {waited:#?}"
    );
    assert!(asking.is_empty(), "still asking: {asking:#?}");
}
