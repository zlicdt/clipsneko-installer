use super::*;

#[test]
fn parse_hostname_i_single_ip() {
    assert_eq!(parse_hostname_i("192.168.1.100\n"), vec!["192.168.1.100"]);
}

#[test]
fn parse_hostname_i_multiple_ips() {
    assert_eq!(
        parse_hostname_i("192.168.1.100 10.0.0.1\n"),
        vec!["192.168.1.100", "10.0.0.1"]
    );
}

#[test]
fn parse_hostname_i_empty() {
    assert!(parse_hostname_i("").is_empty());
    assert!(parse_hostname_i("\n").is_empty());
    assert!(parse_hostname_i("   \n").is_empty());
}

#[test]
fn parse_hostname_i_trims_whitespace() {
    assert_eq!(
        parse_hostname_i("  192.168.1.100   10.0.0.1  \n"),
        vec!["192.168.1.100", "10.0.0.1"]
    );
}

#[test]
fn parse_default_route_normal() {
    let input = "default via 192.168.1.1 dev eth0 proto dhcp metric 100\n";
    assert_eq!(
        parse_default_route(input),
        Some(("192.168.1.1".to_string(), "eth0".to_string()))
    );
}

#[test]
fn parse_default_route_no_route() {
    assert_eq!(parse_default_route(""), None);
    assert_eq!(parse_default_route("\n"), None);
}

#[test]
fn parse_default_route_missing_dev() {
    assert_eq!(parse_default_route("default via 192.168.1.1\n"), None);
}

#[test]
fn parse_default_route_missing_via() {
    assert_eq!(parse_default_route("default dev eth0\n"), None);
}

#[test]
fn parse_default_route_first_line_only() {
    let input = "default via 192.168.1.1 dev eth0\ndefault via 10.0.0.1 dev wlan0\n";
    assert_eq!(
        parse_default_route(input),
        Some(("192.168.1.1".to_string(), "eth0".to_string()))
    );
}

#[test]
fn parse_default_route_extra_fields() {
    let input = "default via fe80::1 dev eth0 proto static metric 100 pref medium\n";
    assert_eq!(
        parse_default_route(input),
        Some(("fe80::1".to_string(), "eth0".to_string()))
    );
}

// --- background check / loading-dialog state machine ---

use crossterm::event::KeyModifiers;
use std::sync::mpsc;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// A step with a fake worker channel: the sender half pre-loaded with the
/// given messages, the receiver half installed as the in-flight check. The
/// sender is returned so tests can keep the channel open (dropping it would
/// look like a dead worker to `tick`).
fn checking_step(
    messages: Vec<Result<RecheckResult, String>>,
) -> (NetworkStep, mpsc::Sender<Result<RecheckResult, String>>) {
    let (sender, receiver) = mpsc::channel();
    for msg in messages {
        sender.send(msg).unwrap();
    }
    let mut step = NetworkStep::new();
    step.checking = true;
    step.receiver = Some(receiver);
    (step, sender)
}

#[test]
fn checking_is_modal_and_swallows_all_keys() {
    let (mut step, _sender) = checking_step(Vec::new());
    let mut state = InstallerState::default();
    assert!(step.has_modal());
    for code in [
        KeyCode::Enter,
        KeyCode::Esc,
        KeyCode::Char('r'),
        KeyCode::Tab,
    ] {
        assert!(matches!(
            step.handle_key(key(code), &mut state).unwrap(),
            StepAction::None
        ));
    }
    // The swallowed keys must not have changed the in-flight check.
    assert!(step.checking);
}

#[test]
fn tick_waits_quietly_while_worker_runs() {
    let (mut step, _sender) = checking_step(Vec::new());
    let mut state = InstallerState::default();
    assert!(matches!(step.tick(&mut state).unwrap(), StepAction::None));
    assert!(step.checking);
    assert_eq!(step.spinner, 1);
}

#[test]
fn tick_applies_successful_result() {
    let (mut step, _sender) = checking_step(vec![Ok(RecheckResult {
        connected: true,
        ips: vec!["192.168.1.100".to_string()],
        route: Some(("192.168.1.1".to_string(), "eth0".to_string())),
    })]);
    let mut state = InstallerState::default();
    assert!(matches!(step.tick(&mut state).unwrap(), StepAction::None));
    assert!(!step.checking);
    assert!(step.receiver.is_none());
    assert!(state.network_ok);
    assert_eq!(step.ips, vec!["192.168.1.100".to_string()]);
    assert_eq!(step.gateway.as_deref(), Some("192.168.1.1"));
    assert_eq!(step.interface.as_deref(), Some("eth0"));
}

#[test]
fn tick_clears_route_when_absent() {
    let (mut step, _sender) = checking_step(vec![Ok(RecheckResult {
        connected: false,
        ips: Vec::new(),
        route: None,
    })]);
    step.gateway = Some("stale".to_string());
    step.interface = Some("stale0".to_string());
    let mut state = InstallerState {
        network_ok: true,
        ..InstallerState::default()
    };
    step.tick(&mut state).unwrap();
    assert!(!state.network_ok);
    assert_eq!(step.gateway, None);
    assert_eq!(step.interface, None);
}

#[test]
fn tick_propagates_worker_failure_as_fatal() {
    let (mut step, _sender) = checking_step(vec![Err("boom".to_string())]);
    let mut state = InstallerState::default();
    let err = step.tick(&mut state).err().unwrap();
    assert!(err.to_string().contains("boom"));
    assert!(!step.checking);
    assert!(step.receiver.is_none());
}
