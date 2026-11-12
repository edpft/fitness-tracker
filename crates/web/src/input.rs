//! The input template (#665): every page that takes input, top to bottom, is
//! a back link and the page's name, then one card per thing it records, each
//! a form whose fields ask one thing apiece.
//!
//! **Agreed on the operator's own schedule on 2026-11-01**, from the mocks of
//! the page #671 builds, on #614's stylesheet. *"These designs are too busy"*
//! was the first answer, so the template has no subtitles and no hints: a
//! field is a label and what it takes.
//!
//! **Each form is folded behind a button** until it is wanted, so a page of
//! them reads as what each card holds.
//!
//! **A refused post costs nothing that was typed.** The page comes back with
//! the form as it was posted and the reason under the field it is about, so
//! fixing one field does not mean typing the others again.

use maud::{DOCTYPE, Markup, html};

/// Why a post was refused, and the field it is about.
///
/// `field` is the name the field posts under, or `None` for a refusal of the
/// form as a whole, which is said above its button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub field: Option<&'static str>,
    pub reason: String,
}

impl Refusal {
    /// A refusal of one field.
    pub const fn of(field: &'static str, reason: String) -> Self {
        Self {
            field: Some(field),
            reason,
        }
    }

    /// A refusal of the form as a whole.
    pub const fn whole(reason: String) -> Self {
        Self {
            field: None,
            reason,
        }
    }
}

/// The whole page: a back link and its name, then `body`.
pub fn page(here: &str, title: &str, body: &Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (crate::page::head(title, &[])) }
            body {
                main {
                    (crate::page::menu(here))
                    header.detail {
                        a href="/" aria-label="Home" { "‹" }
                        h1 { (title) }
                    }
                    (body)
                }
            }
        }
    }
}

/// One field: its label, what it takes, and the reason it was refused if the
/// last post was refused over it.
///
/// `control` is the input or select, which posts under `name` and carries the
/// `id` the label points at.
pub fn field(
    id: &str,
    name: &str,
    label: &str,
    control: &Markup,
    refused: Option<&Refusal>,
) -> Markup {
    let reason = refused
        .filter(|refusal| refusal.field == Some(name))
        .map(|refusal| refusal.reason.as_str());
    html! {
        div.field.refused[reason.is_some()] {
            label for=(id) { (label) }
            (control)
            @if let Some(reason) = reason {
                span.reason { (reason) }
            }
        }
    }
}

/// A field of several rows, the equipment's increments, bands and steps:
/// a label over them and the reason under them, as a single field has.
///
/// `name` is what a refusal of the rows names, since each row posts under
/// names of its own.
pub fn group(name: &str, label: &str, rows: &Markup, refused: Option<&Refusal>) -> Markup {
    let reason = refused
        .filter(|refusal| refusal.field == Some(name))
        .map(|refusal| refusal.reason.as_str());
    html! {
        fieldset.field.refused[reason.is_some()] {
            legend { (label) }
            (rows)
            @if let Some(reason) = reason {
                span.reason { (reason) }
            }
        }
    }
}

/// A word with its first letter in capitals.
pub fn title(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// The reason a whole form was refused, where it is shown: above its button.
pub fn whole(refused: Option<&Refusal>) -> Markup {
    html! {
        @if let Some(Refusal { field: None, reason }) = refused {
            p.reason { (reason) }
        }
    }
}

/// A form folded behind one button, so a page of them reads as what each
/// card holds; open where the last post was this form's and was refused.
pub fn folded(label: &str, refused: Option<&Refusal>, form: &Markup) -> Markup {
    html! {
        details.folded open[refused.is_some()] {
            summary { (label) }
            (form)
        }
    }
}

/// The page when the store could not be read.
pub fn unreadable(here: &str, title: &str, error: &str) -> Markup {
    page(
        here,
        title,
        &html! { p.failed { "the store could not be read: " (error) } },
    )
}

#[cfg(test)]
mod tests {
    use maud::html;

    use super::{Refusal, field, whole};

    /// **The reason is under the field it is about, and nowhere else.**
    #[test]
    fn a_refusal_is_said_beside_its_field() {
        let refused = Refusal::of(
            "loaded",
            "front squat is loaded on its own implement".to_owned(),
        );
        let control = html! { input id="squat-loaded" name="loaded" value="dumbbell"; };

        let at_fault = field(
            "squat-loaded",
            "loaded",
            "Loaded by",
            &control,
            Some(&refused),
        )
        .into_string();
        assert!(at_fault.contains(r#"class="field refused""#), "{at_fault}");
        assert!(
            at_fault.contains("front squat is loaded on its own implement"),
            "{at_fault}"
        );
        assert!(at_fault.contains(r#"value="dumbbell""#), "{at_fault}");

        let other = html! { input id="squat-from" name="from"; };
        let elsewhere = field("squat-from", "from", "From", &other, Some(&refused)).into_string();
        assert!(!elsewhere.contains("refused"), "{elsewhere}");
        assert!(!elsewhere.contains("own implement"), "{elsewhere}");
        assert!(whole(Some(&refused)).into_string().is_empty());
    }

    /// **A refusal of the whole form is said once, above its button.**
    #[test]
    fn a_refusal_of_the_form_is_not_pinned_to_a_field() {
        let refused = Refusal::whole("the store could not be written".to_owned());
        let control = html! { input id="squat-from" name="from"; };

        let shown = field("squat-from", "from", "From", &control, Some(&refused)).into_string();
        assert!(!shown.contains("refused"), "{shown}");
        assert!(
            whole(Some(&refused))
                .into_string()
                .contains("the store could not be written")
        );
    }
}
