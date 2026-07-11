//! Password strength estimation and zeroizing secret storage.

use zeroize::Zeroize;

const COMMON_PASSWORDS: &[&str] = &[
    "123456",
    "12345678",
    "admin",
    "letmein",
    "password",
    "password1",
    "qwerty",
];

/// Coarse password-strength level used only for user feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordStrength {
    Weak,
    Fair,
    Good,
    Strong,
}

/// Estimate password strength without enforcing a minimum strength.
pub fn password_strength(password: &str) -> PasswordStrength {
    if password.is_empty()
        || COMMON_PASSWORDS
            .iter()
            .any(|common| password.eq_ignore_ascii_case(common))
    {
        return PasswordStrength::Weak;
    }

    let length = password.chars().count();
    let classes = [
        password.chars().any(char::is_lowercase),
        password.chars().any(char::is_uppercase),
        password.chars().any(|c| c.is_ascii_digit()),
        password
            .chars()
            .any(|c| !c.is_alphanumeric() && !c.is_whitespace()),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();

    let score = usize::from(length >= 8)
        + usize::from(length >= 12)
        + usize::from(length >= 16)
        + usize::from(classes >= 2)
        + usize::from(classes >= 3)
        + usize::from(classes == 4);

    match score {
        0..=1 => PasswordStrength::Weak,
        2..=3 => PasswordStrength::Fair,
        4..=5 => PasswordStrength::Good,
        _ => PasswordStrength::Strong,
    }
}

/// In-memory secret which never implements `Debug` and zeroizes its buffer.
#[derive(Default)]
pub struct SecretString(String);

impl SecretString {
    /// Wrap a password in zeroizing storage.
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// Borrow the secret for stdin-only handoff or validation.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    /// Append one input character to the secret.
    pub fn push(&mut self, character: char) {
        self.0.push(character);
    }

    /// Remove the final input character, if present.
    pub fn pop(&mut self) {
        self.0.pop();
    }

    /// Zeroize and empty the secret immediately.
    pub fn clear(&mut self) {
        self.0.zeroize();
    }

    /// Whether the secret is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_covers_empty_short_mixed_and_common_passwords() {
        assert_eq!(password_strength(""), PasswordStrength::Weak);
        assert_eq!(password_strength("abc"), PasswordStrength::Weak);
        assert_eq!(password_strength("password"), PasswordStrength::Weak);
        assert_eq!(
            password_strength("alllowercaseonly"),
            PasswordStrength::Fair
        );
        assert_eq!(password_strength("MixedCase123"), PasswordStrength::Good);
        assert_eq!(
            password_strength("Long-Mixed_Case123"),
            PasswordStrength::Strong
        );
    }

    #[test]
    fn secret_can_be_cleared_and_reused() {
        let mut secret = SecretString::new("temporary".to_string());
        secret.clear();
        assert!(secret.is_empty());
        secret.push('x');
        assert_eq!(secret.expose_secret(), "x");
        secret.pop();
        assert!(secret.is_empty());
    }
}
