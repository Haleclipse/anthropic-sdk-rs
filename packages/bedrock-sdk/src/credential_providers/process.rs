//! Port of `@aws-sdk/credential-provider-process` 3.972.x `fromProcess`.
//!
//! `exec` runs the command through the shell with the chain's environment, as
//! Node's `child_process.exec` runs it with `process.env`. Its 1 MiB
//! `maxBuffer` is not ported, and stdin is closed rather than an open pipe
//! (Node's), so a command that reads stdin sees its end instead of waiting.

use std::process::Stdio;

use serde_json::Value;

use super::js::{date_from_value, js_trim};
use super::shared_ini_file_loader::{ParsedIniData, get_profile_name, parse_known_files};
use super::{Credentials, CredentialsProviderError, Environment, Init};

/// `fromProcess(init)()`.
pub(crate) async fn from_process(init: &Init) -> Result<Credentials, CredentialsProviderError> {
    let profiles = parse_known_files(&init.env).await;
    let profile_name = get_profile_name(init.profile.as_deref(), &init.env);
    resolve_process_credentials(&profile_name, &profiles, &init.env).await
}

/// `resolveProcessCredentials`. Every failure is a provider error that lets
/// the chain move on, as npm wraps them all.
pub(crate) async fn resolve_process_credentials(
    profile_name: &str,
    profiles: &ParsedIniData,
    env: &Environment,
) -> Result<Credentials, CredentialsProviderError> {
    let Some(profile) = profiles.get(profile_name) else {
        return Err(CredentialsProviderError::new(format!(
            "Profile {profile_name} could not be found in shared credentials file."
        )));
    };
    let Some(credential_process) = profile.get("credential_process") else {
        return Err(CredentialsProviderError::new(format!(
            "Profile {profile_name} did not contain credential_process."
        )));
    };
    let stdout = exec(credential_process, env)
        .await
        .map_err(CredentialsProviderError::new)?;
    let data: Value = serde_json::from_str(js_trim(&stdout)).map_err(|_| {
        CredentialsProviderError::new(format!(
            "Profile {profile_name} credential_process returned invalid JSON."
        ))
    })?;
    get_validated_process_credentials(profile_name, &data).map_err(CredentialsProviderError::new)
}

/// `getValidatedProcessCredentials`.
fn get_validated_process_credentials(
    profile_name: &str,
    data: &Value,
) -> Result<Credentials, String> {
    if data.get("Version").and_then(Value::as_f64) != Some(1.0) {
        return Err(format!(
            "Profile {profile_name} credential_process did not return Version 1."
        ));
    }
    let (Some(access_key_id), Some(secret_access_key)) =
        (data.get("AccessKeyId"), data.get("SecretAccessKey"))
    else {
        return Err(format!(
            "Profile {profile_name} credential_process returned invalid credentials."
        ));
    };
    let expiration = data.get("Expiration").filter(|value| js_truthy(value));
    let expiration = expiration.and_then(date_from_value);
    if expiration.is_some_and(|expiration| expiration < std::time::SystemTime::now()) {
        return Err(format!(
            "Profile {profile_name} credential_process returned expired credentials."
        ));
    }
    Ok(Credentials {
        access_key_id: js_string(access_key_id),
        secret_access_key: js_string(secret_access_key),
        session_token: data
            .get("SessionToken")
            .filter(|value| js_truthy(value))
            .map(js_string),
        expiration,
    })
}

/// A JSON value's JS truthiness.
fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// `String(value)` for the values a key field can hold.
fn js_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `child_process.exec(command)`: stdout on success; Node's error message
/// (`Command failed: <command>\n<stderr>`) otherwise. The shell is the one
/// Node's `normalizeSpawnArguments` picks.
async fn exec(command: &str, env: &Environment) -> Result<String, String> {
    #[cfg(windows)]
    let (shell, mut child) = {
        // `process.env.comspec || 'cmd.exe'`.
        let shell = env.truthy("ComSpec").unwrap_or("cmd.exe").to_owned();
        let mut child = tokio::process::Command::new(&shell);
        // `/^(?:.*\\)?cmd(?:\.exe)?$/i`: cmd takes the command quoted and
        // verbatim (`windowsVerbatimArguments`), any other shell `-c`.
        let file = shell
            .rsplit('\\')
            .next()
            .unwrap_or(&shell)
            .to_ascii_lowercase();
        if file == "cmd" || file == "cmd.exe" {
            child
                .args(["/d", "/s", "/c"])
                .raw_arg(format!("\"{command}\""));
        } else {
            child.arg("-c").arg(command);
        }
        (shell, child)
    };
    #[cfg(not(windows))]
    let (shell, mut child) = {
        let shell = if cfg!(target_os = "android") {
            "/system/bin/sh"
        } else {
            "/bin/sh"
        }
        .to_owned();
        let mut child = tokio::process::Command::new(&shell);
        child.arg("-c").arg(command);
        (shell, child)
    };
    child
        .env_clear()
        .envs(env.iter())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = child
        .output()
        .await
        .map_err(|error| format!("spawn {shell} {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Command failed: {command}\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
