//! Port of `@smithy/shared-ini-file-loader` 4.x: the profile name, the shared
//! config and credentials file paths, and their parser.
//!
//! npm caches every file it reads for the life of the process (`readFile`'s
//! `filePromises`, cleared only with `ignoreCache`). This port reads the files
//! each time the chain resolves, so a changed file is seen by the next lookup.

use std::collections::HashMap;
use std::path::PathBuf;

use super::Environment;
use super::js::{is_js_whitespace, js_trim};

pub(crate) const ENV_PROFILE: &str = "AWS_PROFILE";
const DEFAULT_PROFILE: &str = "default";
const ENV_CONFIG_PATH: &str = "AWS_CONFIG_FILE";
const ENV_CREDENTIALS_PATH: &str = "AWS_SHARED_CREDENTIALS_FILE";
/// `IniSectionType`'s values.
const SECTION_TYPES: [&str; 3] = ["profile", "sso-session", "services"];
/// `profileNameBlockList`.
const PROFILE_NAME_BLOCK_LIST: [&str; 2] = ["__proto__", "profile __proto__"];

/// One section's keys, `[subsection.]name` → value.
pub(crate) type Section = HashMap<String, String>;
/// Section name → keys.
pub(crate) type ParsedIniData = HashMap<String, Section>;

/// `getProfileName`: `init.profile || process.env.AWS_PROFILE || "default"`.
pub(crate) fn get_profile_name(profile: Option<&str>, env: &Environment) -> String {
    profile
        .filter(|profile| !profile.is_empty())
        .or_else(|| env.truthy(ENV_PROFILE))
        .unwrap_or(DEFAULT_PROFILE)
        .to_owned()
}

/// `getHomeDir`: `HOME`, `USERPROFILE`, `HOMEDRIVE` + `HOMEPATH`, then
/// `os.homedir()`.
pub(crate) fn get_home_dir(env: &Environment) -> PathBuf {
    if let Some(home) = env.truthy("HOME") {
        return home.into();
    }
    if let Some(profile) = env.truthy("USERPROFILE") {
        return profile.into();
    }
    if let Some(path) = env.truthy("HOMEPATH") {
        // A destructuring default: only an unset `HOMEDRIVE` takes it.
        let drive = env
            .var("HOMEDRIVE")
            .map_or_else(|| format!("C:{}", std::path::MAIN_SEPARATOR), str::to_owned);
        return format!("{drive}{path}").into();
    }
    // `os.homedir()`, which the injected environment has no stand-in for.
    // Deprecated before Rust 1.87, the SDK's minimum is 1.85.
    #[allow(deprecated)]
    std::env::home_dir().unwrap_or_default()
}

/// `getConfigFilepath`.
fn get_config_filepath(env: &Environment) -> PathBuf {
    env.truthy(ENV_CONFIG_PATH).map_or_else(
        || get_home_dir(env).join(".aws").join("config"),
        PathBuf::from,
    )
}

/// `getCredentialsFilepath`.
fn get_credentials_filepath(env: &Environment) -> PathBuf {
    env.truthy(ENV_CREDENTIALS_PATH).map_or_else(
        || get_home_dir(env).join(".aws").join("credentials"),
        PathBuf::from,
    )
}

/// `iniLine.split(/(^|\s)[;#]/)[0]`: the line up to its first comment, a `;`
/// or `#` at the start or after whitespace.
fn strip_comment(line: &str) -> &str {
    let mut previous_is_space = true;
    for (index, c) in line.char_indices() {
        if previous_is_space && (c == ';' || c == '#') {
            return &line[..index];
        }
        previous_is_space = is_js_whitespace(c);
    }
    line
}

/// `prefixKeyRegex`, `/^([\w-]+)\s(["'])?([\w-@\+\.%:/]+)\2$/`: a section
/// name such as `profile dev` or `sso-session "my sso"`, split into its prefix
/// and name. `\w` is ASCII here, as in a JS regex without the `u` flag.
fn split_prefixed_section(section_name: &str) -> Option<(&str, &str)> {
    let prefix_end = section_name
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .filter(|&end| end > 0)?;
    let (prefix, rest) = section_name.split_at(prefix_end);
    let separator = rest.chars().next().filter(|&c| is_js_whitespace(c))?;
    let rest = &rest[separator.len_utf8()..];
    let name = match rest.chars().next() {
        Some(quote @ ('"' | '\'')) => rest[1..].strip_suffix(quote)?,
        _ => rest,
    };
    let is_name_char = |c: char| {
        c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '@' | '+' | '.' | '%' | ':' | '/')
    };
    (!name.is_empty() && name.chars().all(is_name_char)).then_some((prefix, name))
}

/// `parseIni`. A section named `__proto__` is an error, as npm throws.
pub(crate) fn parse_ini(ini_data: &str) -> Result<ParsedIniData, String> {
    let mut map = ParsedIniData::new();
    let mut current_section: Option<String> = None;
    let mut current_sub_section: Option<String> = None;
    for ini_line in ini_data.split('\n') {
        let ini_line = ini_line.strip_suffix('\r').unwrap_or(ini_line);
        let trimmed_line = js_trim(strip_comment(ini_line));
        let is_section = trimmed_line.starts_with('[') && trimmed_line.ends_with(']');
        if is_section && trimmed_line.len() >= 2 {
            current_section = None;
            current_sub_section = None;
            let section_name = &trimmed_line[1..trimmed_line.len() - 1];
            match split_prefixed_section(section_name) {
                Some((prefix, name)) => {
                    if SECTION_TYPES.contains(&prefix) {
                        current_section = Some(format!("{prefix}.{name}"));
                    }
                }
                None => current_section = Some(section_name.to_owned()),
            }
            if PROFILE_NAME_BLOCK_LIST.contains(&section_name) {
                return Err(format!("Found invalid profile name \"{section_name}\""));
            }
        } else if let Some(section) = &current_section {
            let Some(index_of_equals_sign) = trimmed_line.find('=').filter(|&index| index > 0)
            else {
                continue;
            };
            let name = js_trim(&trimmed_line[..index_of_equals_sign]);
            let value = js_trim(&trimmed_line[index_of_equals_sign + 1..]);
            if value.is_empty() {
                current_sub_section = Some(name.to_owned());
            } else {
                if current_sub_section.is_some()
                    && ini_line.trim_start_matches(is_js_whitespace) == ini_line
                {
                    current_sub_section = None;
                }
                let key = match &current_sub_section {
                    Some(sub_section) => format!("{sub_section}.{name}"),
                    None => name.to_owned(),
                };
                map.entry(section.clone())
                    .or_default()
                    .insert(key, value.to_owned());
            }
        }
    }
    Ok(map)
}

/// `getConfigData`: the config file's `[profile x]` sections as `x`, its
/// other prefixed sections under their full key, and `[default]`.
fn get_config_data(data: ParsedIniData) -> ParsedIniData {
    let mut config = ParsedIniData::new();
    if let Some(default) = data.get(DEFAULT_PROFILE) {
        config.insert(DEFAULT_PROFILE.to_owned(), default.clone());
    }
    for (key, value) in data {
        let Some((prefix, name)) = key.split_once('.') else {
            continue;
        };
        if !SECTION_TYPES.contains(&prefix) {
            continue;
        }
        let key = if prefix == "profile" {
            name.to_owned()
        } else {
            key.clone()
        };
        config.insert(key, value);
    }
    config
}

/// The two parsed files `loadSharedConfigFiles` returns.
#[derive(Debug, Default)]
pub(crate) struct SharedConfigFiles {
    pub config_file: ParsedIniData,
    pub credentials_file: ParsedIniData,
}

/// `readFile(path, "utf8")`, decoded as Node does.
async fn read_file(path: &std::path::Path) -> Option<String> {
    let bytes = tokio::fs::read(path).await.ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// `loadSharedConfigFiles`: a file that cannot be read or parsed is empty.
pub(crate) async fn load_shared_config_files(env: &Environment) -> SharedConfigFiles {
    let home_dir = get_home_dir(env);
    // `path.join(homeDir, filepath.slice(2))`, which keeps a further leading
    // `/` under the home directory where `PathBuf::join` would replace it.
    let resolve = |path: PathBuf| match path.to_str().and_then(|path| path.strip_prefix("~/")) {
        Some(relative) => home_dir.join(relative.trim_start_matches('/')),
        None => path,
    };
    let config_path = resolve(get_config_filepath(env));
    let credentials_path = resolve(get_credentials_filepath(env));
    let (config, credentials) = tokio::join!(read_file(&config_path), read_file(&credentials_path));
    SharedConfigFiles {
        config_file: config
            .and_then(|text| parse_ini(&text).ok())
            .map(get_config_data)
            .unwrap_or_default(),
        credentials_file: credentials
            .and_then(|text| parse_ini(&text).ok())
            .unwrap_or_default(),
    }
}

/// `parseKnownFiles`: the config file's sections, each overlaid with the
/// credentials file's section of the same name (`mergeConfigFiles`).
pub(crate) async fn parse_known_files(env: &Environment) -> ParsedIniData {
    let files = load_shared_config_files(env).await;
    let mut merged = files.config_file;
    for (key, values) in files.credentials_file {
        merged.entry(key).or_default().extend(values);
    }
    merged
}

/// `fromSharedConfigFiles`' profile: the credentials file's keys overlaid
/// with the config file's (`preferredFile: "config"`).
pub(crate) fn config_preferred_profile(files: &SharedConfigFiles, profile: &str) -> Section {
    let mut merged = files
        .credentials_file
        .get(profile)
        .cloned()
        .unwrap_or_default();
    if let Some(config) = files.config_file.get(profile) {
        merged.extend(config.clone());
    }
    merged
}
