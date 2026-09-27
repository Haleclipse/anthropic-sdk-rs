// Rust-only host extension of TS `readEnv()`: an embedding host installs the
// environment it owns. The source is process-wide, so this suite runs in its
// own test binary and never leaks into the others.

use anthropic_sdk::internal::env::{read_env, set_env_source};
use anthropic_sdk::{Anthropic, ClientOptions, LogLevel};

fn host_env(name: &str) -> Option<String> {
    match name {
        "ANTHROPIC_API_KEY" => Some(" host-key ".to_owned()),
        "ANTHROPIC_BASE_URL" => Some("https://host.example/api".to_owned()),
        "ANTHROPIC_LOG" => Some("debug".to_owned()),
        _ => None,
    }
}

#[test]
fn installed_env_source_backs_every_environment_fallback() {
    assert!(set_env_source(host_env).is_ok());
    assert!(
        set_env_source(|_| None).is_err(),
        "only the first installation takes effect"
    );

    // The host's environment replaces the real one, trimmed like TS readEnv.
    assert_eq!(read_env("ANTHROPIC_API_KEY").as_deref(), Some("host-key"));
    assert_eq!(read_env("PATH"), None);

    let client = Anthropic::new(ClientOptions::default()).unwrap();
    assert_eq!(client.api_key(), Some("host-key"));
    assert_eq!(client.auth_token(), None);
    assert_eq!(client.base_url(), "https://host.example/api");
    assert_eq!(client.log_level(), LogLevel::Debug);

    // Explicit options still win over the environment.
    let client = Anthropic::new(ClientOptions {
        api_key: Some("explicit".to_owned()),
        base_url: Some("https://explicit.example".to_owned()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(client.api_key(), Some("explicit"));
    assert_eq!(client.base_url(), "https://explicit.example");
}
