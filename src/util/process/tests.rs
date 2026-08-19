use super::*;

#[test]
fn is_root_returns_bool() {
    // We cannot assert the exact value (depends on who runs the test),
    // but it must be a deterministic bool and not panic.
    let _ = is_root();
}

#[test]
fn privileged_command_as_root_has_no_sudo() {
    // When root, the command should be the program directly.
    // We can only verify this when the test runs as root.
    if is_root() {
        let cmd = privileged_command("mount");
        assert_eq!(cmd.get_program(), std::ffi::OsStr::new("mount"));
    }
}

#[test]
fn privileged_command_as_user_has_sudo() {
    // When not root, the command should be `sudo -- <program>`.
    if !is_root() {
        let cmd = privileged_command("mount");
        assert_eq!(cmd.get_program(), std::ffi::OsStr::new("sudo"));
    }
}

#[test]
fn privileged_command_with_env_as_user_uses_env_after_sudo() {
    if !is_root() {
        let cmd = privileged_command_with_env("pacstrap", &[("LC_ALL", "C")]);
        assert_eq!(cmd.get_program(), std::ffi::OsStr::new("sudo"));
        let args: Vec<String> = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--", "/usr/bin/env", "LC_ALL=C", "pacstrap"]);
    }
}

#[test]
fn privileged_command_with_env_as_root_applies_env_directly() {
    if is_root() {
        let cmd = privileged_command_with_env("pacstrap", &[("LC_ALL", "C")]);
        assert_eq!(cmd.get_program(), std::ffi::OsStr::new("pacstrap"));
        let envs: Vec<(String, Option<String>)> = cmd
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert!(envs.contains(&("LC_ALL".to_string(), Some("C".to_string()))));
    }
}
