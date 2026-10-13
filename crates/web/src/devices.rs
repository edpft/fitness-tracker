//! The devices page (#427): every device, when it was in use and where each
//! date came from, and a form for each thing the operator can say about one.
//!
//! **Hypermedia, and nothing else.** Every form posts one statement; the answer
//! is the register as it now stands, which htmx swaps in, or the whole page
//! where there is no htmx. What a statement means, whether it can be made and
//! what the register becomes are the use case's (§ 19): this parses what was
//! typed into the domain's types and renders what comes back.

use std::{collections::BTreeSet, sync::Arc};

use axum::{
    Form, Router,
    extract::State,
    http::HeaderMap,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use configurator::store::Store;
use domain::{
    device::{AddedDevice, Basis, Device, DeviceName, DeviceStatement, LastUsed, Measurand},
    normalised::OperatorZone,
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

/// What every request reads.
pub struct Register {
    pub store: Store,
    pub zone: OperatorZone,
}

pub fn routes(register: Arc<Register>) -> Router {
    Router::new()
        .route("/devices", get(page))
        .route("/devices/name", post(name))
        .route("/devices/first-used", post(first_used))
        .route("/devices/last-used", post(last_used))
        .route("/devices/sends", post(sends))
        .route("/devices/same-device", post(same_device))
        .route("/devices/add", post(add))
        .with_state(register)
}

/// A form's fields, in the order they were sent. A list rather than a map
/// because a device's measures are one field sent once per box ticked.
type Fields = Form<Vec<(String, String)>>;

fn field<'a>(fields: &'a [(String, String)], name: &str) -> Result<&'a str, String> {
    fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
        .ok_or_else(|| format!("the form sent no {name}"))
}

fn parsed<T: std::str::FromStr>(fields: &[(String, String)], name: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    field(fields, name)?
        .parse()
        .map_err(|error: T::Err| error.to_string())
}

async fn page(State(register): State<Arc<Register>>) -> Markup {
    whole(&register, None).await
}

async fn name(
    State(register): State<Arc<Register>>,
    headers: HeaderMap,
    Form(fields): Fields,
) -> impl IntoResponse {
    let statement = || -> Result<DeviceStatement, String> {
        Ok(DeviceStatement::Named {
            device: parsed(&fields, "device")?,
            name: parsed::<DeviceName>(&fields, "name")?,
        })
    };
    state(&register, &headers, statement()).await
}

async fn first_used(
    State(register): State<Arc<Register>>,
    headers: HeaderMap,
    Form(fields): Fields,
) -> impl IntoResponse {
    let statement = || -> Result<DeviceStatement, String> {
        Ok(DeviceStatement::FirstUsed {
            device: parsed(&fields, "device")?,
            on: parsed::<Date>(&fields, "on")?,
        })
    };
    state(&register, &headers, statement()).await
}

async fn last_used(
    State(register): State<Arc<Register>>,
    headers: HeaderMap,
    Form(fields): Fields,
) -> impl IntoResponse {
    let statement = || -> Result<DeviceStatement, String> {
        Ok(DeviceStatement::LastUsed {
            device: parsed(&fields, "device")?,
            last: parsed::<LastUsed>(&fields, "last")?,
        })
    };
    state(&register, &headers, statement()).await
}

async fn sends(
    State(register): State<Arc<Register>>,
    headers: HeaderMap,
    Form(fields): Fields,
) -> impl IntoResponse {
    let statement = || -> Result<DeviceStatement, String> {
        Ok(DeviceStatement::Sends {
            device: parsed(&fields, "device")?,
            what: measurands(&fields)?,
        })
    };
    state(&register, &headers, statement()).await
}

/// Every box ticked in a form's `measures`.
fn measurands(fields: &[(String, String)]) -> Result<BTreeSet<Measurand>, String> {
    fields
        .iter()
        .filter(|(key, _)| key == "measures")
        .map(|(_, value)| value.parse::<Measurand>())
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|error| error.to_string())
}

async fn same_device(
    State(register): State<Arc<Register>>,
    headers: HeaderMap,
    Form(fields): Fields,
) -> impl IntoResponse {
    let statement = || -> Result<DeviceStatement, String> {
        Ok(DeviceStatement::SameDevice {
            device: parsed(&fields, "device")?,
            other: parsed(&fields, "other")?,
        })
    };
    state(&register, &headers, statement()).await
}

async fn add(
    State(register): State<Arc<Register>>,
    headers: HeaderMap,
    Form(fields): Fields,
) -> impl IntoResponse {
    let added = || -> Result<AddedDevice, String> {
        let measures = measurands(&fields)?;
        AddedDevice::new(
            parsed(&fields, "name")?,
            measures,
            parsed(&fields, "first_used")?,
            LastUsed::StillInUse,
        )
        .map_err(|error| error.to_string())
    };
    let notice = match added() {
        Ok(added) => configurator::devices::add(&register.store, &register.zone, added)
            .await
            .err()
            .map(|error| error.to_string()),
        Err(error) => Some(error),
    };
    answer(&register, &headers, notice).await
}

/// Keep a statement, and answer with the register it leaves or why not.
async fn state(
    register: &Register,
    headers: &HeaderMap,
    statement: Result<DeviceStatement, String>,
) -> Html<String> {
    let notice = match statement {
        Ok(statement) => configurator::devices::state(&register.store, &register.zone, statement)
            .await
            .err()
            .map(|error| error.to_string()),
        Err(error) => Some(error),
    };
    answer(register, headers, notice).await
}

/// The register for htmx to swap in, or the page where a browser posted the
/// form itself.
async fn answer(register: &Register, headers: &HeaderMap, notice: Option<String>) -> Html<String> {
    if headers.contains_key("hx-request") {
        Html(section(register, notice.as_deref()).await.into_string())
    } else {
        Html(whole(register, notice.as_deref()).await.into_string())
    }
}

async fn whole(register: &Register, notice: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Devices" }
                style { (crate::page::STYLE) (STYLE) }
                script src="/htmx.min.js" {}
            }
            body hx-boost="true" {
                main {
                    p { a href="/" { "next" } }
                    h1 { "Devices" }
                    (section(register, notice).await)
                }
            }
        }
    }
}

/// The part every form replaces.
async fn section(register: &Register, notice: Option<&str>) -> Markup {
    let devices = configurator::devices::devices(&register.store, &register.zone).await;
    html! {
        div #register {
            @if let Some(notice) = notice { p.failed { (notice) } }
            @match &devices {
                Ok(devices) => {
                    @for device in devices { (row(device, devices)) }
                    (adding())
                }
                Err(error) => p.failed { (error) },
            }
        }
    }
}

fn row(device: &Device, devices: &[Device]) -> Markup {
    let Some(identifier) = device.identifier() else {
        return html! {};
    };
    let id = identifier.to_string();
    let swap = |path: &'static str| (path, "#register", "outerHTML");
    html! {
        details.device {
            summary {
                strong { (device.name().value) }
                " "
                span.period { (device.first_used().value) " – " (device.last_used().value) }
            }
            (facts(device))
            @let (path, target, how) = swap("/devices/name");
            form method="post" action=(path) hx-post=(path) hx-target=(target) hx-swap=(how) {
                input type="hidden" name="device" value=(id);
                label { "Name " input name="name" required value=(device.name().value); }
                button { "Rename" }
            }
            @let (path, target, how) = swap("/devices/first-used");
            form method="post" action=(path) hx-post=(path) hx-target=(target) hx-swap=(how) {
                input type="hidden" name="device" value=(id);
                label { "First used " input type="date" name="on" required value=(device.first_used().value); }
                button { "Set" }
            }
            @let (path, target, how) = swap("/devices/last-used");
            form method="post" action=(path) hx-post=(path) hx-target=(target) hx-swap=(how) {
                input type="hidden" name="device" value=(id);
                label {
                    "Last used "
                    input type="date" name="last" required value=[last_date(device.last_used().value)];
                }
                button { "Set" }
            }
            form method="post" action=(path) hx-post=(path) hx-target=(target) hx-swap=(how) {
                input type="hidden" name="device" value=(id);
                input type="hidden" name="last" value="still-in-use";
                button { "Still in use" }
            }
            @let (path, target, how) = swap("/devices/sends");
            form method="post" action=(path) hx-post=(path) hx-target=(target) hx-swap=(how) {
                input type="hidden" name="device" value=(id);
                fieldset {
                    legend { "Sends to another device" }
                    @for measurand in Measurand::ALL {
                        label.choice {
                            input type="checkbox" name="measures" value=(measurand.as_str())
                                checked[device.sends().value.contains(&measurand)];
                            " " (measurand)
                        }
                    }
                }
                button { "Set" }
            }
            @let (path, target, how) = swap("/devices/same-device");
            form method="post" action=(path) hx-post=(path) hx-target=(target) hx-swap=(how) {
                input type="hidden" name="device" value=(id);
                label {
                    "Same device as "
                    select name="other" required {
                        option value="" { "—" }
                        @for other in devices.iter().filter(|other| !std::ptr::eq(*other, device)) {
                            @if let Some(theirs) = other.identifier() {
                                option value=(theirs) {
                                    (other.name().value) " (" (other.first_used().value) ")"
                                }
                            }
                        }
                    }
                }
                button { "Merge" }
            }
        }
    }
}

/// What the register says about a device, and where each of it came from.
fn facts(device: &Device) -> Markup {
    html! {
        dl {
            dt { "Measures" }
            dd { (listed(device.measures(), "nothing, in any recording")) }
            dt { "Sends to another device" }
            dd {
                (listed(&device.sends().value, "nothing"))
                " " span.basis { (basis(device.sends().basis)) }
            }
            dt { "First used" }
            dd { (device.first_used().value) " " span.basis { (basis(device.first_used().basis)) } }
            dt { "Last used" }
            dd { (device.last_used().value) " " span.basis { (basis(device.last_used().basis)) } }
            dt { "Identified by" }
            dd {
                @for (index, identifier) in device.identifiers().iter().enumerate() {
                    @if index > 0 { ", " }
                    (identifier)
                }
            }
        }
    }
}

/// A list of measurands as a page says it.
fn listed(measurands: &BTreeSet<Measurand>, none: &str) -> Markup {
    html! {
        @if measurands.is_empty() { (none) }
        @else {
            @for (index, measurand) in measurands.iter().enumerate() {
                @if index > 0 { ", " }
                (measurand)
            }
        }
    }
}

fn adding() -> Markup {
    let path = "/devices/add";
    html! {
        details.device {
            summary { strong { "Add a device no recording names" } }
            form.add method="post" action=(path) hx-post=(path) hx-target="#register" hx-swap="outerHTML" {
                label { "Name " input name="name" required; }
                fieldset {
                    legend { "Measures" }
                    @for measurand in Measurand::ALL {
                        label.choice {
                            input type="checkbox" name="measures" value=(measurand.as_str());
                            " " (measurand)
                        }
                    }
                }
                label { "First used " input type="date" name="first_used" required; }
                p.basis { "It is added as still in use; say when it was last used once it is in the list." }
                button { "Add" }
            }
        }
    }
}

/// The date a "last used" field opens on, where there is one.
const fn last_date(last: LastUsed) -> Option<Date> {
    match last {
        LastUsed::On(date) => Some(date),
        LastUsed::StillInUse => None,
    }
}

fn basis(basis: Basis) -> String {
    match basis {
        Basis::Recorded { source } => format!("from {source}"),
        Basis::Stated { at } => format!("as you said on {}", at.strftime("%Y-%m-%d")),
    }
}

const STYLE: &str = "
h1 { font-size: 1.4rem; }
details.device { border-top: 1px solid var(--muted); padding: 0.5rem 0; }
summary { cursor: pointer; }
.period, .basis { color: var(--muted); }
dl { display: grid; grid-template-columns: max-content 1fr; gap: 0.15rem 0.75rem; }
dt { color: var(--muted); }
dd { margin: 0; }
form { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: end; margin: 0.5rem 0; }
form button { width: auto; padding: 6px 12px; }
form.add { flex-direction: column; align-items: stretch; }
fieldset { display: flex; flex-wrap: wrap; gap: 0.25rem 1rem; }
input, select { font: inherit; }
";
