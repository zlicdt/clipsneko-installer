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
}

impl UiLang {
    /// POSIX locale code used for `setlocale` and `bindtextdomain` lookups.
    pub fn code(self) -> &'static str {
        match self {
            UiLang::En => "en_US.UTF-8",
            UiLang::ZhCn => "zh_CN.UTF-8",
        }
    }

    fn catalog_directory(self) -> &'static str {
        match self {
            UiLang::En => "en",
            UiLang::ZhCn => "zh_CN",
        }
    }
}

/// Initialize gettext with the given UI language. Must be called once at
/// startup; calling it again switches the active translation at runtime.
///
/// The ClipsNeko ISO build is responsible for generating both
/// `en_US.UTF-8` and `zh_CN.UTF-8` and installing both release catalogs.
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
