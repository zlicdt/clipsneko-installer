//! i18n bootstrap. Wraps gettext-rs and exposes the `t!()` macro used at every
//! user-facing call site.

use anyhow::{Context, Result};
use gettextrs::{
    bind_textdomain_codeset, bindtextdomain, gettext, setlocale, textdomain, LocaleCategory,
};

const DOMAIN: &str = "clipsneko-installer";

/// Supported installer UI languages. The variant order is the order shown in
/// the language picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLang {
    En,
    ZhCn,
    ZhTw,
}

impl UiLang {
    /// POSIX locale code used for `setlocale` and `bindtextdomain` lookups.
    pub fn code(self) -> &'static str {
        match self {
            UiLang::En => "en_US.UTF-8",
            UiLang::ZhCn => "zh_CN.UTF-8",
            UiLang::ZhTw => "zh_TW.UTF-8",
        }
    }

    /// Human-readable label shown in the picker itself (kept in English so the
    /// picker is usable before any translation is active).
    pub fn label(self) -> &'static str {
        match self {
            UiLang::En => "English",
            UiLang::ZhCn => "简体中文",
            UiLang::ZhTw => "繁體中文",
        }
    }
}

/// Initialize gettext with the given UI language. Must be called once at
/// startup; calling it again switches the active translation at runtime.
///
/// The ClipsNeko ISO build is responsible for generating both
/// `en_US.UTF-8` and `zh_CN.UTF-8`, so on the supported runtime this call
/// always succeeds. A failure is treated as a defensive fallback: the
/// caller (e.g. the language step) logs it via `tracing` and re-applies
/// English so the installer keeps working.
pub fn set_language(lang: UiLang) -> Result<()> {
    // setlocale must succeed for gettext to pick the right .mo file.
    setlocale(LocaleCategory::LcAll, lang.code()).with_context(|| {
        format!(
            "setlocale({}) failed; locale not generated on this system",
            lang.code()
        )
    })?;

    let locale_dir = std::env::var("CLIPSNEKO_LOCALEDIR")
        .unwrap_or_else(|_| env!("CLIPSNEKO_DEV_LOCALEDIR").to_string());
    bindtextdomain(DOMAIN, &locale_dir)
        .with_context(|| format!("bindtextdomain({}, {}) failed", DOMAIN, locale_dir))?;
    bind_textdomain_codeset(DOMAIN, "UTF-8").context("bind_textdomain_codeset failed")?;
    textdomain(DOMAIN).context("textdomain failed")?;
    Ok(())
}

/// Translate a string via the active gettext catalog.
pub fn translate(s: &str) -> String {
    gettext(s)
}

/// Translate a literal at the call site. Usage: `t!("Welcome")`. The input
/// must be a `&'static str` so xgettext can extract it from source.
#[macro_export]
macro_rules! t {
    ($s:expr) => {
        $crate::i18n::translate($s)
    };
}
