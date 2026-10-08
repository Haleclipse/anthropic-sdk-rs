//! Port of `@aws-sdk/credential-provider-ini` 3.972.x `fromIni`.
//!
//! `resolveProfileData` tries a profile's kinds in npm's order. Static keys
//! and `credential_process` resolve; a profile that assumes a role, uses a
//! web identity token, SSO or a login session reports that it is not
//! resolved yet, and stops the chain rather than letting another source
//! answer for it.

use super::shared_ini_file_loader::{get_profile_name, parse_known_files, ParsedIniData, Section};
use super::{process, Credentials, CredentialsProviderError, Init};

/// `fromIni(init)()`.
pub(crate) async fn from_ini(init: &Init) -> Result<Credentials, CredentialsProviderError> {
    let profiles = parse_known_files(&init.env).await;
    let profile_name = get_profile_name(init.profile.as_deref(), &init.env);
    resolve_profile_data(&profile_name, &profiles, init).await
}

/// `resolveProfileData`.
async fn resolve_profile_data(
    profile_name: &str,
    profiles: &ParsedIniData,
    init: &Init,
) -> Result<Credentials, CredentialsProviderError> {
    let data = profiles.get(profile_name);
    let not_resolved_yet = |kind: &str| {
        Err(CredentialsProviderError::stop(format!(
            "Profile {profile_name} uses {kind}, which is not resolved yet."
        )))
    };
    if data.is_some_and(is_assume_role_profile) {
        return not_resolved_yet("role assumption (role_arn)");
    }
    if let Some(credentials) = data.and_then(static_credentials) {
        return Ok(credentials);
    }
    if data.is_some_and(is_web_identity_profile) {
        return not_resolved_yet("a web identity token (web_identity_token_file)");
    }
    if data.is_some_and(|data| data.contains_key("credential_process")) {
        // `fromProcess({ ...options, profile })`, on the files already read.
        return process::resolve_process_credentials(profile_name, profiles, &init.env).await;
    }
    if data.is_some_and(is_sso_profile) {
        return not_resolved_yet("SSO");
    }
    if data.is_some_and(|data| {
        data.get("login_session")
            .is_some_and(|value| !value.is_empty())
    }) {
        return not_resolved_yet("a login session (login_session)");
    }
    Err(CredentialsProviderError::new(format!(
        "Could not resolve credentials using profile: [{profile_name}] in configuration/credentials file(s)."
    )))
}

/// `isAssumeRoleProfile`: `role_arn` with exactly one of `source_profile` and
/// `credential_source`. INI values are always strings, so npm's `typeof`
/// checks are presence checks.
fn is_assume_role_profile(data: &Section) -> bool {
    data.contains_key("role_arn")
        && (data.contains_key("source_profile") != data.contains_key("credential_source"))
}

/// `isStaticCredsProfile` / `resolveStaticCredentials`.
fn static_credentials(data: &Section) -> Option<Credentials> {
    Some(Credentials {
        access_key_id: data.get("aws_access_key_id")?.clone(),
        secret_access_key: data.get("aws_secret_access_key")?.clone(),
        session_token: data.get("aws_session_token").cloned(),
        expiration: None,
    })
}

/// `isWebIdentityProfile`.
fn is_web_identity_profile(data: &Section) -> bool {
    data.contains_key("web_identity_token_file") && data.contains_key("role_arn")
}

/// `isSsoProfile`.
fn is_sso_profile(data: &Section) -> bool {
    [
        "sso_start_url",
        "sso_account_id",
        "sso_session",
        "sso_region",
        "sso_role_name",
    ]
    .iter()
    .any(|key| data.contains_key(*key))
}
