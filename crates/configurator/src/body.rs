//! Body weight and body composition, read together on one page (#786).

use application::next::Scope;

use crate::wiring::WiringError;

/// What the page reads: what body weight's charts read, and what body
/// composition's do.
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn collects() -> Result<Scope, WiringError> {
    let weight = crate::body_weight::collects()?;
    let composition = crate::composition::collects()?;
    Ok(Scope::new(
        weight.parts().iter().chain(composition.parts()).cloned(),
    ))
}
