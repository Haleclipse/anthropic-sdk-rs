//! Maps to: `@azure/msal-common` 16.4.0 `src/request/ScopeSet.ts`. A scope
//! set is a `Vec` of its scopes in insertion order.

use std::collections::HashSet;

use super::auth_error::MsalError;
use super::constants::{OFFLINE_ACCESS_SCOPE, OIDC_SCOPES};
use crate::azure_identity::js::is_js_whitespace;

/// Maps to: `ScopeSet.ts:30-47` `new ScopeSet(scopes)`: entries trimmed,
/// empty ones dropped, duplicates (case-sensitive) merged in insertion order;
/// none left is `empty_input_scopes_error`.
pub(crate) fn scope_set<S: AsRef<str>>(scopes: &[S]) -> Result<Vec<String>, MsalError> {
    let mut set: Vec<String> = Vec::new();
    for scope in scopes {
        let scope = scope.as_ref().trim_matches(is_js_whitespace);
        if !scope.is_empty() && !set.iter().any(|existing| existing == scope) {
            set.push(scope.to_owned());
        }
    }
    if set.is_empty() {
        return Err(MsalError::client_configuration("empty_input_scopes_error"));
    }
    Ok(set)
}

/// `ScopeSet.ts:56-60` `ScopeSet.fromString`.
pub(crate) fn scope_set_from_string(scopes: &str) -> Result<Vec<String>, MsalError> {
    scope_set(&scopes.split(' ').collect::<Vec<_>>())
}

/// `ScopeSet.ts:67-81` `ScopeSet.createSearchScopes`, for non-empty scopes.
pub(crate) fn create_search_scopes(scopes: &[String]) -> Result<Vec<String>, MsalError> {
    let mut set = scope_set(scopes)?;
    if contains_only_oidc_scopes(&set) {
        set.retain(|scope| scope != OFFLINE_ACCESS_SCOPE);
    } else {
        remove_oidc_scopes(&mut set);
    }
    Ok(set)
}

/// `ScopeSet.ts:88-95` `containsScope`: compared lowercased.
fn contains_scope(set: &[String], scope: &str) -> bool {
    let wanted = scope.to_lowercase();
    !scope.is_empty()
        && set
            .join(" ")
            .to_lowercase()
            .split(' ')
            .map(|entry| entry.trim_matches(is_js_whitespace))
            .any(|entry| !entry.is_empty() && entry == wanted)
}

/// `ScopeSet.ts:101-110` `containsScopeSet`.
pub(crate) fn contains_scope_set(set: &[String], other: &[String]) -> bool {
    !other.is_empty()
        && set.len() >= other.len()
        && other.iter().all(|scope| contains_scope(set, scope))
}

/// `ScopeSet.ts:115-124` `containsOnlyOIDCScopes`.
pub(crate) fn contains_only_oidc_scopes(set: &[String]) -> bool {
    OIDC_SCOPES
        .iter()
        .filter(|scope| contains_scope(set, scope))
        .count()
        == set.len()
}

/// `ScopeSet.ts:167-171` `removeOIDCScopes`: exact matches only.
pub(crate) fn remove_oidc_scopes(set: &mut Vec<String>) {
    set.retain(|scope| !OIDC_SCOPES.contains(&scope.as_str()));
}

/// `ScopeSet.ts:195-218` `intersectingScopeSets`, with `other` already
/// stripped as its OIDC step leaves it: the lowercased union is smaller than
/// the two sizes together.
pub(crate) fn intersecting_scope_sets(set: &[String], other: &[String]) -> bool {
    let union: HashSet<String> = other
        .iter()
        .chain(set)
        .map(|scope| scope.to_lowercase())
        .collect();
    union.len() < set.len() + other.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ScopeSet.ts` and `CacheManager.ts:634-680`: a token replaces the
    /// cached ones its scopes intersect, compared case-insensitively.
    #[test]
    fn scope_sets_match_official_set_rules() {
        assert_eq!(scope_set(&[" a ", "a", "", "B"]).unwrap(), ["a", "B"]);
        assert_eq!(
            create_search_scopes(&["a".to_owned(), "openid".to_owned()]).unwrap(),
            ["a"]
        );
        assert_eq!(
            create_search_scopes(&["openid".to_owned(), "offline_access".to_owned()]).unwrap(),
            ["openid"]
        );
        let set = |scopes: &[&str]| {
            scopes
                .iter()
                .map(|scope| (*scope).to_owned())
                .collect::<Vec<_>>()
        };
        assert!(contains_scope_set(&set(&["A", "b"]), &set(&["a"])));
        assert!(!contains_scope_set(&set(&["a"]), &set(&["a", "b"])));
        assert!(intersecting_scope_sets(&set(&["A", "c"]), &set(&["a"])));
        assert!(!intersecting_scope_sets(&set(&["b"]), &set(&["a"])));
    }
}
