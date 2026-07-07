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
