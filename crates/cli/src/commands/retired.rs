//! Answering a retired v1 spelling with its exact v2 replacement.

use wayfind_core::{
    error::{ErrorToken, Rejection},
    retired,
};

use crate::error::ShellError;

/// Refuse a retired v1 spelling, naming its v2 replacement.
///
/// `prefix` is the retired command's fixed words (`["ticket", "claim"]`);
/// `rest` is whatever trailing words the operator typed after it. The
/// [`Rejection`] this builds is always [`ErrorToken::RetiredCommand`] in
/// practice — `prefix` names a path this module always keeps registered in
/// [`wayfind_core::retired::REPLACEMENTS`] — but the fallback keeps this
/// function total without reaching for `.unwrap()` or `.expect()`.
pub fn refuse(prefix: &[&str], rest: &[String]) -> ShellError {
    let path: Vec<&str> = prefix
        .iter()
        .copied()
        .chain(rest.iter().map(String::as_str))
        .collect();
    let rejection = retired::retired(&path).unwrap_or_else(|| Rejection::new(ErrorToken::Usage));
    ShellError::from(rejection)
}
