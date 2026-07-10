//! Target-system UTF-8 locale enumeration from `/etc/locale.gen`.

use anyhow::{bail, Context, Result};

const LOCALE_GEN_PATH: &str = "/etc/locale.gen";

/// Read every available UTF-8 locale from the Live ISO's locale.gen file.
pub fn list_utf8_locales() -> Result<Vec<String>> {
    let text = std::fs::read_to_string(LOCALE_GEN_PATH)
        .with_context(|| format!("reading {LOCALE_GEN_PATH}"))?;
    let locales = parse_utf8_locales(&text);
    if locales.is_empty() {
        bail!("{LOCALE_GEN_PATH} contains no UTF-8 locales");
    }
    Ok(locales)
}

/// Parse commented and enabled locale.gen entries, retaining one copy of each
/// UTF-8 locale in file order.
pub fn parse_utf8_locales(text: &str) -> Vec<String> {
    let mut locales = Vec::new();
    for line in text.lines() {
        let entry = line.trim().strip_prefix('#').unwrap_or(line.trim()).trim();
        let mut fields = entry.split_whitespace();
        let (Some(locale), Some(charmap)) = (fields.next(), fields.next()) else {
            continue;
        };
        if charmap.eq_ignore_ascii_case("UTF-8")
            && locale.to_ascii_uppercase().ends_with(".UTF-8")
            && !locales.iter().any(|existing| existing == locale)
        {
            locales.push(locale.to_string());
        }
    }
    locales
}

#[cfg(test)]
mod tests;
