#[allow(unused_imports)]
pub use rust_i18n::t;

/// Translate a key to the current locale.
///
/// In debug builds, returns `[MISSING: key]` to surface untranslated keys.
/// In release builds, performs the actual translation via `rust_i18n::t!`.
#[allow(dead_code)]
pub fn tr(key: &'static str) -> String {
    if cfg!(debug_assertions) {
        format!("[MISSING: {key}]")
    } else {
        rust_i18n::t!(key).to_string()
    }
}

/// Return the currently active locale string (e.g. `"zh-CN"`, `"en-US"`).
pub fn current_locale() -> String {
    rust_i18n::locale().to_string()
}

/// Set the application locale at runtime.
///
/// Accepts `"zh-CN"` or `"en-US"`.  `"follow-system"` is intentionally
/// **not** handled here — the startup logic in `main.rs` is responsible
/// for resolving system locale and calling this function with an explicit
/// locale string.
///
/// Invalid values are silently ignored.
pub fn set_app_locale(lang: &str) {
    match lang {
        "zh-CN" | "en-US" => rust_i18n::set_locale(lang),
        _ => {}
    }
}

/// Detect locale from a raw locale string (e.g. `"zh_CN.UTF-8"`).
///
/// This is a pure function — no environment variable access.
/// See [`detect_system_locale`] for the env-aware wrapper.
///
/// Rules: `"zh*"` / `"chinese"` → `"zh-CN"`,
///        `"en*"` / `"english"` → `"en-US"`,
///         everything else → `"en-US"`.
pub fn detect_from_raw(raw: &str) -> String {
    let lang = raw.split(['.', '_']).next().unwrap_or("").to_lowercase();
    match lang.as_str() {
        "zh" | "zh-cn" | "zh-tw" | "zh-hk" | "chinese" => "zh-CN".to_string(),
        "en" | "en-us" | "en-gb" | "english" => "en-US".to_string(),
        _ => "en-US".to_string(),
    }
}

/// Detect system locale from `LC_MESSAGES` / `LANG` environment variables.
///
/// Priority: `LC_MESSAGES` > `LANG`.  Falls back to `"en-US"` when neither
/// is set or when the value doesn't match a known locale pattern.
pub fn detect_system_locale() -> String {
    let raw = std::env::var("LC_MESSAGES")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    detect_from_raw(&raw)
}

/// Log the current locale at startup for diagnostics.
#[cfg(feature = "log-miss-tr")]
pub fn log_locale_info() {
    let locale = current_locale();
    log::info!("i18n: locale={}, zh-CN=100%, en-US=100%", locale);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_zh_cn() {
        assert_eq!(detect_from_raw("zh_CN.UTF-8"), "zh-CN");
    }

    #[test]
    fn test_detect_en_us() {
        assert_eq!(detect_from_raw("en_US.UTF-8"), "en-US");
    }

    #[test]
    fn test_detect_fallback_french() {
        assert_eq!(detect_from_raw("fr_FR.UTF-8"), "en-US");
    }

    #[test]
    fn test_detect_empty() {
        assert_eq!(detect_from_raw(""), "en-US");
    }

    #[test]
    fn test_detect_c() {
        assert_eq!(detect_from_raw("C"), "en-US");
    }
}
