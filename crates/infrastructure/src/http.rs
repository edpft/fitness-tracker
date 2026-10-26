//! Every request this program makes, and what is kept when its answer was not
//! the one expected (#164).
//!
//! The operator, 2026-09-18: *"if we receive a response we don't expect, to
//! any request we make, we should store the bytes so that we can investigate
//! later."* And on 2026-10-16: record it once, at the HTTP client, and keep
//! everything — sign-in pages and `Set-Cookie` headers included.
//!
//! **Unexpected is decided by whoever reads the answer**, because only it
//! knows: a 404 is the answer `detail_if_served` hopes for and a fault from a
//! sign-in form, and Garmin's widget answers 200 whether or not it signed in.
//! So an [`Answer`] hides its status and body until it is [`read`](Answer::read),
//! and an answer the reader refuses is kept, the reader's own error beside it.
//!
//! **Kept where the store is**, wherever the program last opened it: the
//! adapters that make requests are built before, and apart from, any store,
//! and some — the credential check, the holiday calendars — never hold one.
//! Opening the store is what turns the keeping on, so there is one place it
//! happens rather than one per adapter.
//!
//! **Keeping is not allowed to fail the request.** What the reader refused is
//! the error that matters; a store that cannot take the row loses the row, and
//! the reader's error still reaches the operator.

use std::{
    fmt::Display,
    path::{Path, PathBuf},
    sync::RwLock,
};

use application::SourceError;
use reqwest::{Method, RequestBuilder, StatusCode, Url, header::HeaderMap};
use sqlx::{
    ConnectOptions as _, Connection as _,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};

/// The store an unexpected answer is kept in, once one has been opened.
static KEPT_IN: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Keep every unexpected answer from here on in the store at `path`.
///
/// Called by [`crate::store::connect`], once the store's schema is current.
pub fn keep_unexpected_responses_in(path: &Path) {
    if let Ok(mut kept_in) = KEPT_IN.write() {
        *kept_in = Some(path.to_owned());
    }
}

/// What a source answered, as the reader of an [`Answer`] sees it.
#[derive(Debug)]
pub struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Reply {
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The body as text, any byte that is not UTF-8 replaced rather than
    /// refused: the bytes themselves are what is kept.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// An answer, received in full and not yet read.
#[derive(Debug)]
#[must_use = "an answer is only kept if it is read and refused"]
pub struct Answer {
    method: Method,
    url: Url,
    reply: Reply,
}

/// Send a request and take the whole of its answer.
///
/// # Errors
///
/// [`reqwest::Error`] if the request cannot be built or sent, or its body
/// cannot be received. Nothing was answered, so nothing is kept.
pub async fn send(request: RequestBuilder) -> Result<Answer, reqwest::Error> {
    let (client, request) = request.build_split();
    let request = request?;
    let method = request.method().clone();
    let response = client.execute(request).await?;

    let url = response.url().clone();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await?.to_vec();

    Ok(Answer {
        method,
        url,
        reply: Reply {
            status,
            headers,
            body,
        },
    })
}

/// [`send`], read as a source: no answer at all is a source that is
/// unavailable.
///
/// # Errors
///
/// [`SourceError::Unavailable`] where [`send`] fails.
pub async fn ask(request: RequestBuilder) -> Result<Answer, SourceError> {
    send(request)
        .await
        .map_err(|error| SourceError::Unavailable {
            detail: error.to_string(),
        })
}

impl Answer {
    /// Read the answer, keeping it if `read` refuses it.
    ///
    /// # Errors
    ///
    /// Whatever `read` returned, after the answer has been kept.
    pub async fn read<T, E: Display>(
        self,
        read: impl FnOnce(&Reply) -> Result<T, E>,
    ) -> Result<T, E> {
        match read(&self.reply) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.keep(&error.to_string()).await;
                Err(error)
            }
        }
    }

    /// Write this answer to the store, if one has been opened.
    async fn keep(&self, detail: &str) {
        let Some(path) = KEPT_IN.read().ok().and_then(|kept_in| kept_in.clone()) else {
            return;
        };
        // A reader that says nothing still refused; the column will not be empty.
        let detail = if detail.is_empty() { "refused" } else { detail };
        if let Err(error) = self.write(&path, detail).await {
            eprintln!(
                "an unexpected answer from {} could not be kept: {error}",
                self.url
            );
        }
    }

    async fn write(&self, path: &Path, detail: &str) -> Result<(), sqlx::Error> {
        let mut connection = SqliteConnectOptions::new()
            .filename(path)
            .journal_mode(SqliteJournalMode::Wal)
            .connect()
            .await?;

        let method = self.method.as_str();
        let url = self.url.as_str();
        let status = self.reply.status.to_string();
        let succeeded = i64::from(self.reply.status.is_success());
        let headers = header_lines(&self.reply.headers);
        let body = self.reply.body.as_slice();
        let answered_at = jiff::Timestamp::now().to_string();

        sqlx::query!(
            r#"
            INSERT INTO unexpected_response
                (method, url, status, succeeded, headers, body, detail, answered_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            method,
            url,
            status,
            succeeded,
            headers,
            body,
            detail,
            answered_at
        )
        .execute(&mut connection)
        .await?;

        connection.close().await
    }
}

/// The headers as they would be written on the wire, value bytes verbatim.
fn header_lines(headers: &HeaderMap) -> Vec<u8> {
    let mut lines = Vec::new();
    for (name, value) in headers {
        lines.extend_from_slice(name.as_str().as_bytes());
        lines.extend_from_slice(b": ");
        lines.extend_from_slice(value.as_bytes());
        lines.extend_from_slice(b"\r\n");
    }
    lines
}
