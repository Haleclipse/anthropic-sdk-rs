//! Rust-only test support, no TS counterpart.
//!
//! Runs one test in a child copy of the test binary with an exact
//! environment. The TS SDK tests environment defaults by assigning to
//! `process.env`; Rust cannot: `std::env::set_var` is `unsafe` in edition
//! 2024 because it races with any concurrent reader, including C code in the
//! same process. A child process gets its environment at spawn time instead,
//! so the test process never writes its own.
//!
//! Shared by the core and provider test crates through `#[path]`.

use std::process::Command;

/// Set in the child to the libtest name of the test it runs.
const CHILD_MARKER: &str = "ANTHROPIC_SDK_RS_ENV_CHILD";

/// What a child keeps from the parent: enough to run the binary (search and
/// library paths), create temp files and bind localhost mock servers, plus
/// test tooling. Nothing SDK- or provider-specific passes through, so host
/// variables such as `ANTHROPIC_API_KEY` or `AWS_PROFILE` cannot change a
/// test's outcome.
const PASS_THROUGH: &[&str] = &[
    "PATH",
    "LD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "TMPDIR",
    "TMP",
    "TEMP",
    "SYSTEMROOT",
    "RUST_BACKTRACE",
    "LLVM_PROFILE_FILE",
];

/// Runs the calling test in a child process whose environment is exactly
/// `vars` plus [`PASS_THROUGH`].
///
/// In the parent this spawns the child, asserts it ran exactly one test and
/// passed, and returns `true`: the caller returns immediately. In the child
/// it returns `false` and the caller runs its body.
///
/// `module_path` is the caller's `module_path!()` and `test` its function
/// name; together they form the libtest name the child filters on.
pub fn run_in_child_env(module_path: &str, test: &str, vars: &[(&str, &str)]) -> bool {
    // libtest names omit the crate: `anthropic_sdk::client::tests` becomes
    // `client::tests`, and an integration-test crate root has no prefix.
    let name = match module_path.split_once("::") {
        Some((_, rest)) => format!("{rest}::{test}"),
        None => test.to_owned(),
    };

    // libtest runs each test on a thread named after it. A `test` that is not
    // the caller's own name would run some other test under this environment.
    assert_eq!(
        std::thread::current().name(),
        Some(name.as_str()),
        "run_in_child_env was given `{test}`, which is not the calling test"
    );

    match std::env::var(CHILD_MARKER) {
        Ok(requested) if requested == name => return false,
        Ok(requested) => panic!(
            "{CHILD_MARKER}={requested} is set outside the child it names; \
             unset it so `{name}` runs isolated"
        ),
        Err(_) => {}
    }

    let mut command = Command::new(std::env::current_exe().expect("test binary path"));
    command
        .args([name.as_str(), "--exact", "--nocapture", "--test-threads=1"])
        .env_clear();
    for key in PASS_THROUGH {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.env(CHILD_MARKER, &name);
    command.envs(vars.iter().copied());

    let output = command.output().expect("spawn child test process");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "child test `{name}` failed\n--- stdout\n{stdout}\n--- stderr\n{stderr}"
    );
    assert!(
        stdout.contains("1 passed"),
        "child test `{name}` did not run exactly one test\n--- stdout\n{stdout}"
    );
    true
}
