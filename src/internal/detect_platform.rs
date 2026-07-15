// Maps to: TS internal/detect-platform.ts
//
//! Stainless platform header detection.
//!
//! The TypeScript SDK reports JavaScript runtime details (`node`, `deno`,
//! browser, etc.). The Rust SDK reports analogous Rust package/runtime details
//! while preserving the same header names and normalization rules for OS/arch
//! values where they overlap.

use std::collections::HashMap;

use crate::VERSION;

/// Normalized architecture string used by Stainless headers.
pub type Arch = String;
/// Normalized operating-system string used by Stainless headers.
pub type PlatformName = String;

/// Platform properties corresponding to TS `PlatformProperties`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformProperties {
    pub lang: String,
    pub package_version: String,
    pub os: PlatformName,
    pub arch: Arch,
    pub runtime: String,
    pub runtime_version: String,
}

/// Browser detection is not meaningful for the native Rust SDK.
///
/// Maps to TS `isRunningInBrowser()` and always returns `false`.
pub fn is_running_in_browser() -> bool {
    false
}

/// TS-style camelCase alias for [`is_running_in_browser`].
#[allow(non_snake_case)]
pub fn isRunningInBrowser() -> bool {
    is_running_in_browser()
}

/// Normalize architecture names using TS `normalizeArch()` semantics plus Rust
/// target aliases such as `x86`.
pub fn normalize_arch(arch: &str) -> Arch {
    match arch {
        "x32" | "x86" | "i386" | "i586" | "i686" => "x32".to_owned(),
        "x86_64" | "x64" => "x64".to_owned(),
        "arm" => "arm".to_owned(),
        "aarch64" | "arm64" => "arm64".to_owned(),
        "" => "unknown".to_owned(),
        other => format!("other:{other}"),
    }
}

/// TS-style camelCase alias for [`normalize_arch`].
#[allow(non_snake_case)]
pub fn normalizeArch(arch: &str) -> Arch {
    normalize_arch(arch)
}

/// Normalize OS names using TS `normalizePlatform()` semantics plus Rust target
/// aliases such as `macos` and `windows`.
pub fn normalize_platform(platform: &str) -> PlatformName {
    let platform = platform.to_ascii_lowercase();
    match platform.as_str() {
        value if value.contains("ios") => "iOS".to_owned(),
        "android" => "Android".to_owned(),
        "darwin" | "macos" => "MacOS".to_owned(),
        "win32" | "windows" => "Windows".to_owned(),
        "freebsd" => "FreeBSD".to_owned(),
        "openbsd" => "OpenBSD".to_owned(),
        "linux" => "Linux".to_owned(),
        "" => "Unknown".to_owned(),
        other => format!("Other:{other}"),
    }
}

/// TS-style camelCase alias for [`normalize_platform`].
#[allow(non_snake_case)]
pub fn normalizePlatform(platform: &str) -> PlatformName {
    normalize_platform(platform)
}

/// Return Rust platform properties for Stainless headers.
pub fn get_platform_properties() -> PlatformProperties {
    PlatformProperties {
        lang: "rust".to_owned(),
        package_version: VERSION.to_owned(),
        os: normalize_platform(std::env::consts::OS),
        arch: normalize_arch(std::env::consts::ARCH),
        runtime: "rust".to_owned(),
        runtime_version: env!("CARGO_PKG_RUST_VERSION").to_owned(),
    }
}

/// Return canonical Stainless platform headers.
///
/// Maps to TS `getPlatformHeaders()`, with Rust-specific values.
pub fn get_platform_headers() -> HashMap<String, String> {
    let properties = get_platform_properties();
    HashMap::from([
        ("X-Stainless-Lang".to_owned(), properties.lang),
        (
            "X-Stainless-Package-Version".to_owned(),
            properties.package_version,
        ),
        ("X-Stainless-OS".to_owned(), properties.os),
        ("X-Stainless-Arch".to_owned(), properties.arch),
        ("X-Stainless-Runtime".to_owned(), properties.runtime),
        (
            "X-Stainless-Runtime-Version".to_owned(),
            properties.runtime_version,
        ),
    ])
}

/// TS-style camelCase alias for [`get_platform_headers`].
#[allow(non_snake_case)]
pub fn getPlatformHeaders() -> HashMap<String, String> {
    get_platform_headers()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_arch_matches_ts_and_rust_aliases() {
        assert_eq!(normalize_arch("x32"), "x32");
        assert_eq!(normalize_arch("x86"), "x32");
        assert_eq!(normalize_arch("x86_64"), "x64");
        assert_eq!(normalize_arch("x64"), "x64");
        assert_eq!(normalize_arch("arm"), "arm");
        assert_eq!(normalize_arch("aarch64"), "arm64");
        assert_eq!(normalize_arch("arm64"), "arm64");
        assert_eq!(normalize_arch("mips"), "other:mips");
        assert_eq!(normalize_arch(""), "unknown");
        assert_eq!(normalizeArch("wasm32"), "other:wasm32");
    }

    #[test]
    fn normalize_platform_matches_ts_and_rust_aliases() {
        assert_eq!(normalize_platform("darwin"), "MacOS");
        assert_eq!(normalize_platform("macos"), "MacOS");
        assert_eq!(normalize_platform("linux"), "Linux");
        assert_eq!(normalize_platform("win32"), "Windows");
        assert_eq!(normalize_platform("windows"), "Windows");
        assert_eq!(normalize_platform("freebsd"), "FreeBSD");
        assert_eq!(normalize_platform("openbsd"), "OpenBSD");
        assert_eq!(normalize_platform("android"), "Android");
        assert_eq!(normalize_platform("ios-sim"), "iOS");
        assert_eq!(normalize_platform("solaris"), "Other:solaris");
        assert_eq!(normalize_platform(""), "Unknown");
        assert_eq!(normalizePlatform("LINUX"), "Linux");
    }

    #[test]
    fn platform_headers_use_stainless_names_and_rust_values() {
        assert!(!is_running_in_browser());
        assert!(!isRunningInBrowser());

        let headers = getPlatformHeaders();
        assert_eq!(
            headers.get("X-Stainless-Lang").map(String::as_str),
            Some("rust")
        );
        assert_eq!(
            headers
                .get("X-Stainless-Package-Version")
                .map(String::as_str),
            Some(VERSION)
        );
        assert_eq!(
            headers.get("X-Stainless-Runtime").map(String::as_str),
            Some("rust")
        );
        assert!(headers.contains_key("X-Stainless-OS"));
        assert!(headers.contains_key("X-Stainless-Arch"));
        assert!(headers.contains_key("X-Stainless-Runtime-Version"));
    }
}
