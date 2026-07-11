//! i18n bootstrap. Wraps gettext-rs and exposes the `t!()` macro used at every
//! user-facing call site.

use anyhow::{Context, Result};
use gettextrs::{
    bind_textdomain_codeset, bindtextdomain, gettext, setlocale, textdomain, LocaleCategory,
};
use std::path::Path;

const DOMAIN: &str = "clipsneko-installer";

/// Supported installer UI languages. The variant order is the order shown in
/// the language picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLang {
    En,
    ZhCn,
    ZhTw,
    Ja,
    De,
    Ko,
    Ru,
}

impl UiLang {
    /// POSIX locale code used for `setlocale` and `bindtextdomain` lookups.
    pub fn code(self) -> &'static str {
        match self {
            UiLang::En => "en_US.UTF-8",
            UiLang::ZhCn => "zh_CN.UTF-8",
            UiLang::ZhTw => "zh_TW.UTF-8",
            UiLang::Ja => "ja_JP.UTF-8",
            UiLang::De => "de_DE.UTF-8",
            UiLang::Ko => "ko_KR.UTF-8",
            UiLang::Ru => "ru_RU.UTF-8",
        }
    }

    fn catalog_directory(self) -> &'static str {
        match self {
            UiLang::En => "en",
            UiLang::ZhCn => "zh_CN",
            UiLang::ZhTw => "zh_TW",
            UiLang::Ja => "ja",
            UiLang::De => "de",
            UiLang::Ko => "ko",
            UiLang::Ru => "ru",
        }
    }
}

/// Initialize gettext with the given UI language. Must be called once at
/// startup; calling it again switches the active translation at runtime.
///
/// The ClipsNeko ISO build is responsible for generating every supported
/// locale and installing every release catalog.
/// Missing locale/catalog state is a fatal Live ISO invariant violation.
pub fn set_language(lang: UiLang) -> Result<()> {
    // setlocale must succeed for gettext to pick the right .mo file.
    setlocale(LocaleCategory::LcMessages, lang.code()).with_context(|| {
        format!(
            "setlocale({}) failed; locale not generated on this system",
            lang.code()
        )
    })?;

    #[cfg(debug_assertions)]
    let locale_dir = env!("CLIPSNEKO_DEV_LOCALEDIR");
    #[cfg(not(debug_assertions))]
    let locale_dir = "/usr/share/locale";
    let catalog = Path::new(locale_dir)
        .join(lang.catalog_directory())
        .join("LC_MESSAGES")
        .join(format!("{DOMAIN}.mo"));
    if !catalog.is_file() {
        anyhow::bail!("translation catalog is missing: {}", catalog.display());
    }
    bindtextdomain(DOMAIN, locale_dir)
        .with_context(|| format!("bindtextdomain({}, {}) failed", DOMAIN, locale_dir))?;
    bind_textdomain_codeset(DOMAIN, "UTF-8").context("bind_textdomain_codeset failed")?;
    textdomain(DOMAIN).context("textdomain failed")?;
    Ok(())
}

/// Translate a string via the active gettext catalog.
pub fn translate(s: &str) -> String {
    gettext(s)
}

/// Translate a literal message ID at the call site. Usage: `t!("app.title")`.
/// The literal-only macro shape keeps every key visible to xgettext.
#[macro_export]
macro_rules! t {
    ($s:literal) => {
        $crate::i18n::translate($s)
    };
}

#[cfg(test)]
mod tests {
    use super::UiLang;

    #[test]
    fn supported_languages_map_to_release_locales_and_catalogs() {
        let expected = [
            (UiLang::En, "en_US.UTF-8", "en"),
            (UiLang::ZhCn, "zh_CN.UTF-8", "zh_CN"),
            (UiLang::ZhTw, "zh_TW.UTF-8", "zh_TW"),
            (UiLang::Ja, "ja_JP.UTF-8", "ja"),
            (UiLang::De, "de_DE.UTF-8", "de"),
            (UiLang::Ko, "ko_KR.UTF-8", "ko"),
            (UiLang::Ru, "ru_RU.UTF-8", "ru"),
        ];

        for (language, locale, catalog) in expected {
            assert_eq!(language.code(), locale);
            assert_eq!(language.catalog_directory(), catalog);
        }
    }
}
