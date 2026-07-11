//! GeoIP timezone detection through the endpoint already used by the network
//! step. Detection is only a default: callers fall back to UTC when curl or
//! the response does not produce a supported timezone.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::process::Command;

const GEOIP_URL: &str = "http://ip-api.com/json";

#[derive(Deserialize)]
struct GeoIpResponse {
    timezone: Option<String>,
}

/// Ask ip-api.com for the current timezone.
///
/// Network, HTTP, and JSON failures return `None` because timezone selection
/// remains usable with its UTC fallback. Failure to start the ISO-provided
/// `curl` command is a fatal invariant error.
pub fn detect_timezone() -> Result<Option<String>> {
    let output = Command::new("curl")
        .args([
            "--max-time",
            "5",
            "--fail",
            "--silent",
            "--show-error",
            GEOIP_URL,
        ])
        .output()
        .context("running curl for GeoIP timezone detection")?;

    if !output.status.success() {
        tracing::warn!(
            status = %output.status,
            stderr = %String::from_utf8_lossy(&output.stderr).trim(),
            "GeoIP request failed"
        );
        return Ok(None);
    }

    let timezone = parse_timezone_response(&output.stdout).or_else(|| {
        tracing::warn!("GeoIP response did not contain a usable timezone");
        None
    });
    Ok(timezone)
}

fn parse_timezone_response(bytes: &[u8]) -> Option<String> {
    let response: GeoIpResponse = serde_json::from_slice(bytes).ok()?;
    response.timezone.filter(|timezone| !timezone.is_empty())
}

#[cfg(test)]
mod tests {
    use super::parse_timezone_response;

    #[test]
    fn parses_timezone_and_rejects_missing_or_invalid_values() {
        assert_eq!(
            parse_timezone_response(br#"{"timezone":"Asia/Shanghai"}"#),
            Some("Asia/Shanghai".to_string())
        );
        assert_eq!(parse_timezone_response(br#"{"timezone":""}"#), None);
        assert_eq!(parse_timezone_response(br#"{"status":"fail"}"#), None);
        assert_eq!(parse_timezone_response(b"not json"), None);
    }
}
