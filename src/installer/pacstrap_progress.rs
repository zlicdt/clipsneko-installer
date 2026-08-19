//! Pure streaming parser for `pacstrap`/`pacman` output.
//!
//! `pacstrap` runs `pacman` with stdout/stderr attached to our pipes, so
//! pacman is in non-TTY mode. Its stable C-locale markers in that mode are
//! `Packages (N)`, one `... downloading...` line per package archive, one
//! `installing <name>...` line per package, and the phase headers printed by
//! pacman. TTY-style `(n/N)` package counts are intentionally not supported:
//! this installer never gives pacstrap a TTY.
//!
//! The parser is deliberately forgiving: it buffers arbitrary chunk splits,
//! keeps only the final `\r` frame of a line, strips ANSI sequences, ignores
//! unknown lines, and never decreases the reported fraction. Callers still
//! own command success/failure; `finish` is called only after pacstrap exits
//! successfully and returns the full 100% stage fraction.

use std::collections::HashSet;

const INSTALLING_PACKAGES: u8 = 0;
const SYNCING_DATABASES: u8 = 2;
const RESOLVING_DEPENDENCIES: u8 = 4;
const CHECKING_CONFLICTS: u8 = 6;
const PACKAGE_LIST: u8 = 8;
const RETRIEVING_PACKAGES: u8 = 10;
const DOWNLOAD_START: u8 = 10;
const DOWNLOAD_SPAN: u8 = 40;
const CHECKING_KEYRING: u8 = 53;
const CHECKING_INTEGRITY: u8 = 56;
const LOADING_PACKAGE_FILES: u8 = 59;
const CHECKING_FILE_CONFLICTS: u8 = 62;
const PROCESSING_PACKAGE_CHANGES: u8 = 65;
const INSTALL_START: u8 = 65;
const INSTALL_SPAN: u8 = 30;
const RUNNING_HOOKS: u8 = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    AwaitingPacman,
    Downloading,
    Installing,
    Hooks,
}

/// Incremental parser for pacstrap's non-TTY output.
#[derive(Debug)]
pub(crate) struct PacstrapProgressParser {
    pending: Vec<u8>,
    phase: Phase,
    total_packages: Option<usize>,
    download_names: HashSet<String>,
    install_names: HashSet<String>,
    last_fraction: Option<u8>,
}

impl PacstrapProgressParser {
    pub(crate) fn new() -> Self {
        Self {
            pending: Vec::new(),
            phase: Phase::AwaitingPacman,
            total_packages: None,
            download_names: HashSet::new(),
            install_names: HashSet::new(),
            last_fraction: None,
        }
    }

    /// Feed an arbitrary raw chunk from stdout or stderr. Returns every new
    /// package-stage fraction in `0..=100` produced by this chunk, in order.
    /// A chunk may contain many complete lines, so the caller must forward
    /// all returned fractions. Unknown output contributes nothing.
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        if chunk.is_empty() {
            return Vec::new();
        }
        self.pending.extend_from_slice(chunk);
        let mut progress = Vec::new();
        while let Some(newline) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line = self.pending.drain(..newline + 1).collect::<Vec<u8>>();
            if let Some(fraction) = self.process_line(&line) {
                progress.push(fraction);
            }
        }
        progress
    }

    /// Return the final fraction after pacstrap has exited successfully.
    pub(crate) fn finish(self) -> u8 {
        100
    }

    fn process_line(&mut self, line: &[u8]) -> Option<u8> {
        let without_newline = line.strip_suffix(b"\n").unwrap_or(line);
        let frame = without_newline
            .rsplit(|byte| *byte == b'\r')
            .next()
            .unwrap_or(without_newline);
        let stripped = super::strip_ansi(&String::from_utf8_lossy(frame));
        let line = stripped.trim();
        if line.is_empty() {
            return None;
        }

        if line.starts_with("==> Installing packages to") {
            self.phase = Phase::AwaitingPacman;
            return self.set(INSTALLING_PACKAGES);
        }
        if line.starts_with(":: Synchronizing package databases...") {
            return self.set(SYNCING_DATABASES);
        }
        if line.starts_with("resolving dependencies...") {
            return self.set(RESOLVING_DEPENDENCIES);
        }
        if line.starts_with("looking for conflicting packages...") {
            return self.set(CHECKING_CONFLICTS);
        }
        if line.starts_with("Packages (") {
            if let Some(total) = parse_total_packages(line) {
                self.total_packages = Some(total);
                return self.set(PACKAGE_LIST);
            }
        }
        if line.starts_with(":: Retrieving packages...") {
            self.phase = Phase::Downloading;
            return self.set(RETRIEVING_PACKAGES);
        }

        if self.phase == Phase::Downloading {
            if let Some(name) = package_download_name(line) {
                if self.download_names.insert(name.to_string()) {
                    if let Some(total) = self.total_packages {
                        return self.set(download_fraction(self.download_names.len(), total));
                    }
                }
            }
        }

        if line.starts_with("checking keyring...") {
            return self.set(CHECKING_KEYRING);
        }
        if line.starts_with("checking package integrity...") {
            return self.set(CHECKING_INTEGRITY);
        }
        if line.starts_with("loading package files...") {
            return self.set(LOADING_PACKAGE_FILES);
        }
        if line.starts_with("checking for file conflicts...") {
            return self.set(CHECKING_FILE_CONFLICTS);
        }

        if line.starts_with(":: Processing package changes...") {
            self.phase = Phase::Installing;
            return self.set(PROCESSING_PACKAGE_CHANGES);
        }
        if self.phase == Phase::Installing {
            if let Some(name) = install_name(line) {
                if self.install_names.insert(name.to_string()) {
                    if let Some(total) = self.total_packages {
                        return self.set(install_fraction(self.install_names.len(), total));
                    }
                }
            }
        }

        if line.starts_with(":: Running post-transaction hooks...") {
            self.phase = Phase::Hooks;
            return self.set(RUNNING_HOOKS);
        }
        None
    }

    fn set(&mut self, fraction: u8) -> Option<u8> {
        let fraction = fraction.min(100);
        if self.last_fraction.is_none_or(|last| fraction > last) {
            self.last_fraction = Some(fraction);
            Some(fraction)
        } else {
            None
        }
    }
}

fn parse_total_packages(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("Packages (")?;
    let end = rest.find(')')?;
    rest[..end].trim().parse().ok()
}

fn package_download_name(line: &str) -> Option<&str> {
    let name = line.strip_suffix("downloading...")?.trim();
    if name.is_empty() || !name.contains(".pkg.tar") || name.ends_with(".sig") {
        return None;
    }
    Some(name)
}

fn install_name(line: &str) -> Option<&str> {
    let body = line.strip_suffix("...")?;
    for action in ["installing ", "upgrading ", "reinstalling ", "downgrading "] {
        if let Some(name) = body.strip_prefix(action) {
            let name = name.trim();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn download_fraction(downloaded: usize, total: usize) -> u8 {
    if total == 0 {
        return DOWNLOAD_START;
    }
    let downloaded = downloaded.min(total);
    DOWNLOAD_START + ((downloaded * usize::from(DOWNLOAD_SPAN)) / total) as u8
}

fn install_fraction(installed: usize, total: usize) -> u8 {
    if total == 0 {
        return INSTALL_START;
    }
    let installed = installed.min(total);
    INSTALL_START + ((installed * usize::from(INSTALL_SPAN)) / total) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRANSCRIPT: &str = "\
==> Installing packages to /mnt
:: Synchronizing package databases...
 core downloading...
resolving dependencies...
looking for conflicting packages...

Packages (3) demo1-1.0-1  demo2-1.0-1  demo3-1.0-1

Total Download Size:   0.00 MiB

:: Proceed with installation? [Y/n] 
:: Retrieving packages...
 demo1-1.0-1-x86_64.pkg.tar.zst downloading...
 demo1-1.0-1-x86_64.pkg.tar.zst.sig downloading...
 demo2-1.0-1-x86_64.pkg.tar.zst downloading...
 demo3-1.0-1-x86_64.pkg.tar.zst downloading...
checking keyring...
checking package integrity...
loading package files...
checking for file conflicts...
:: Processing package changes...
installing demo1...
installing demo2...
installing demo3...
:: Running post-transaction hooks...
";

    fn parse_with_chunk_size(transcript: &str, chunk_size: usize) -> Vec<u8> {
        let mut parser = PacstrapProgressParser::new();
        let mut fractions = Vec::new();
        for chunk in transcript.as_bytes().chunks(chunk_size) {
            fractions.extend(parser.push(chunk));
        }
        assert_eq!(parser.finish(), 100);
        fractions
    }

    #[test]
    fn full_non_tty_transcript_advances_through_expected_fractions() {
        let expected = [
            0, 2, 4, 6, 8, 10, 23, 36, 50, 53, 56, 59, 62, 65, 75, 85, 95, 96,
        ];
        assert_eq!(parse_with_chunk_size(TRANSCRIPT, usize::MAX), expected);
    }

    #[test]
    fn parser_is_independent_of_chunk_boundaries() {
        let expected = parse_with_chunk_size(TRANSCRIPT, 1);
        for chunk_size in 2..=11 {
            assert_eq!(parse_with_chunk_size(TRANSCRIPT, chunk_size), expected);
        }
    }

    #[test]
    fn signature_downloads_are_ignored_and_progress_never_decreases() {
        let mut parser = PacstrapProgressParser::new();
        assert_eq!(parser.push(b"Packages (1) demo\n"), [8]);
        assert!(parser.push(b"unknown output\n").is_empty());
        assert_eq!(parser.push(b":: Retrieving packages...\n"), [10]);
        assert!(parser
            .push(b"demo-1.0-1-x86_64.pkg.tar.zst.sig downloading...\n")
            .is_empty());
        assert_eq!(
            parser.push(b"demo-1.0-1-x86_64.pkg.tar.zst downloading...\n"),
            [50]
        );
        assert!(parser
            .push(b"demo-1.0-1-x86_64.pkg.tar.zst downloading...\n")
            .is_empty());
        assert_eq!(parser.push(b":: Processing package changes...\n"), [65]);
        assert_eq!(parser.push(b"installing demo...\n"), [95]);
        assert!(parser.push(b"installing demo...\n").is_empty());
        assert_eq!(parser.finish(), 100);
    }

    #[test]
    fn ansi_sequences_do_not_hide_markers() {
        let mut parser = PacstrapProgressParser::new();
        assert_eq!(parser.push(b"\x1b[1mPackages (2) a b\x1b[0m\n"), [8]);
    }

    #[test]
    fn carriage_return_uses_only_the_last_frame() {
        let mut parser = PacstrapProgressParser::new();
        assert!(parser
            .push(b"Packages (9) should-not-parse\rignored\n")
            .is_empty());
        assert_eq!(parser.push(b":: Retrieving packages...\n"), [10]);
        assert!(parser
            .push(b"a-1.0-1-x86_64.pkg.tar.zst downloading...\n")
            .is_empty());
    }
}
