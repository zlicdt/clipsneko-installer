//! Disk partitioning step — two sub-pages inside one wizard step (per
//! `design.md` §4 step 5).
//!
//! **Sub-page A — disk picker:** lists every `disk`-kind block device from
//! `lsblk -J -O -b` (name + human size). Enter opens `cfdisk /dev/<disk>`
//! full-screen (via `sudo` when not root); after it exits the installer runs
//! `partprobe` and re-reads `lsblk`. The user may run cfdisk against multiple
//! disks. The on-screen Next button advances to sub-page B.
//!
//! **Sub-page B — partition role picker:** lists every `part`-kind device on
//! every disk from the latest `lsblk` (name / size / current FSTYPE). Enter
//! pops a small dialog to assign the highlighted partition the **ESP** role
//! (single-select, picking a new one clears the old) or the **Target** role
//! (multi-select; two or more Target partitions enable btrfs RAID at format
//! time, see `design.md` §5), or to cancel. The Next button is enabled only
//! when an ESP is set and the total Target size exceeds 20 GiB. Pressing Next,
//! if any Target currently has a non-empty FSTYPE (will be reformatted as
//! btrfs → data loss) or the ESP partition is not already vfat (will be
//! `mkfs.vfat -F32`'d), shows a single blocking confirmation dialog listing
//! the partitions that will be wiped; a pure-vfat ESP incurs no warning. There
//! is no auto-suggested role assignment and no extra-mount mapping in v0.1.
//!
//! Per the project-wide list style (no "▶" marker — the option's own text is
//! bold+bright to signal selected state), assigned partitions in the partition
//! list and assigned rows inside dialogs are styled bold.

use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::lsblk::{self, BlockDevice};
use crate::util::process::{is_root, privileged_command};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

/// Selected state renders the row's own text in bold + a bright color so it
/// stands out without an extra marker glyph (per user direction).
fn selected_style() -> Style {
    Style::default()
        .add_modifier(Modifier::BOLD)
        .fg(Color::White)
}

/// Which sub-page is currently shown in the disk step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    DiskPicker,
    PartitionAssign,
}

/// The role-assignment dialog (sub-page B): offers the user a choice of ESP,
/// Target, or Cancel for the partition under the cursor.
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
enum RoleOption {
    Esp,
    Target,
    Cancel,
}

impl RoleOption {
    fn all() -> [RoleOption; 3] {
        [RoleOption::Esp, RoleOption::Target, RoleOption::Cancel]
    }
}

/// State for the role-assignment dialog.
#[derive(Debug)]
struct RoleDialog {
    visible: bool,
    cursor: usize,
    /// Partition name this dialog is asking about.
    part: String,
}

/// State for the unified wipe-warning dialog shown on Next in sub-page B.
/// `partitions` carries (name, role) pairs that will be wiped so the user sees
/// exactly what will go away. `confirmed` flips to `true` once the user presses
/// Enter (the step then emits `StepAction::Next`).
#[derive(Debug, Default)]
struct WipeDialog {
    visible: bool,
    confirmed: bool,
    partitions: Vec<(String, String)>,
}

pub struct DiskStep {
    phase: Phase,
    /// Snapshot of disk-kind devices from `lsblk`. Lazily populated in
    /// `activate` (and after each `cfdisk` run).
    disks: Vec<BlockDevice>,
    /// Snapshot of part-kind devices from `lsblk`. Populated when entering
    /// sub-page B (and refreshed whenever the tree is re-read).
    parts: Vec<BlockDevice>,
    /// Cursor for whichever sub-page list is currently visible.
    list_state: ListState,
    role_dialog: RoleDialog,
    wipe_dialog: WipeDialog,
}

impl DiskStep {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            phase: Phase::DiskPicker,
            disks: Vec::new(),
            parts: Vec::new(),
            list_state,
            role_dialog: RoleDialog {
                visible: false,
                cursor: 0,
                part: String::new(),
            },
            wipe_dialog: WipeDialog::default(),
        }
    }

    /// Refresh the block-device snapshot from `lsblk`.
    fn refresh_devices(&mut self) {
        self.disks = lsblk::list_devices();
        self.parts = lsblk::flat_parts(&self.disks)
            .into_iter()
            .cloned()
            .collect();
        tracing::debug!(
            "lsblk refresh: {} disks, {} parts",
            self.disks.len(),
            self.parts.len()
        );
        // Clamp the cursor so it stays in-bounds after the refresh.
        let len = self.current_list_len();
        if len == 0 {
            self.list_state.select(None);
        } else {
            let cur = self.list_state.selected().unwrap_or(0);
            let next = cur.min(len - 1);
            self.list_state.select(Some(next));
        }
    }

    /// Run `partprobe` (privileged) to force the kernel to re-read the
    /// partition table on disks that `cfdisk` may have modified.
    fn partprobe(&self) {
        let status = privileged_command("partprobe").output();
        match status {
            Ok(o) if o.status.success() => tracing::debug!("partprobe ok"),
            Ok(o) => tracing::warn!(
                "partprobe exited non-zero: {}; stderr: {}",
                o.status,
                String::from_utf8_lossy(&o.stderr).trim()
            ),
            Err(e) => tracing::warn!("partprobe spawn failed: {e}"),
        }
    }

    fn current_list_len(&self) -> usize {
        match self.phase {
            Phase::DiskPicker => lsblk::flat_disks(&self.disks).len(),
            Phase::PartitionAssign => self.parts.len(),
        }
    }

    fn move_highlight(&mut self, delta: i32) {
        let len = self.current_list_len() as i32;
        if len == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(len) as usize;
        self.list_state.select(Some(next));
    }

    /// Reset transient dialogs (called on every activate).
    fn reset_dialogs(&mut self) {
        self.role_dialog.visible = false;
        self.role_dialog.cursor = 0;
        self.role_dialog.part.clear();
        self.wipe_dialog.visible = false;
        self.wipe_dialog.confirmed = false;
        self.wipe_dialog.partitions.clear();
    }

    /// Are the current assignments enough to enable Next?
    fn assignments_valid(&self, state: &InstallerState) -> bool {
        state.disk.esp_partition.is_some()
            && !state.disk.target_partitions.is_empty()
            && self.target_total_for_state(state) >= lsblk::TARGET_MIN_BYTES
    }

    /// Sum the byte sizes of state's target partitions using the current
    /// snapshot. Partitions not in the snapshot contribute 0.
    fn target_total_for_state(&self, state: &InstallerState) -> u64 {
        state
            .disk
            .target_partitions
            .iter()
            .filter_map(|name| self.parts.iter().find(|p| &p.name == name).map(|p| p.size))
            .sum()
    }

    // --- role dialog (sub-page B) ---

    fn open_role_dialog(&mut self) {
        if let Some(part) = self.highlighted_part_name() {
            self.role_dialog = RoleDialog {
                visible: true,
                cursor: 0,
                part,
            };
        }
    }

    /// Currently highlighted partition name on sub-page B (or `None`).
    fn highlighted_part_name(&self) -> Option<String> {
        let idx = self.list_state.selected()?;
        self.parts.get(idx).map(|p| p.name.clone())
    }

    /// Apply the role picked in the role dialog to the shared state.
    fn apply_role(&mut self, option: RoleOption, state: &mut InstallerState) {
        let part = self.role_dialog.part.clone();
        match option {
            RoleOption::Esp => {
                if state.disk.esp_partition.as_deref() == Some(part.as_str()) {
                    state.disk.esp_partition = None;
                } else {
                    state.disk.esp_partition = Some(part);
                }
            }
            RoleOption::Target => {
                if let Some(pos) = state.disk.target_partitions.iter().position(|n| n == &part) {
                    state.disk.target_partitions.remove(pos);
                } else {
                    state.disk.target_partitions.push(part);
                }
            }
            RoleOption::Cancel => {}
        }
    }

    // --- wipe dialog (sub-page B Final-N confirmation) ---

    /// Compute the list of partitions that will be wiped given the current
    /// shared-state assignments (Targets with non-empty FSTYPE + ESP if not
    /// vfat). Each entry carries a short reason string for the dialog body.
    fn compute_wipes(&self, state: &InstallerState) -> Vec<(String, String)> {
        compute_wipe_list(&self.parts, state)
    }

    fn open_wipe_dialog(&mut self, state: &InstallerState) {
        self.wipe_dialog = WipeDialog {
            visible: true,
            confirmed: false,
            partitions: self.compute_wipes(state),
        };
    }
}

// --- pure-logic helpers (unit-tested) ---

/// Given the current partition snapshot and shared state, return the list of
/// partitions that will be wiped, paired with a short reason. Pure (no shell
/// out): Targets with a non-empty `fstype` are reformatted as btrfs; the ESP is
/// reformatted as vfat only when its current `fstype` is not already `vfat`.
pub(crate) fn compute_wipe_list(
    parts: &[BlockDevice],
    state: &InstallerState,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for name in &state.disk.target_partitions {
        if let Some(p) = parts.iter().find(|p| &p.name == name) {
            if p.fstype.as_deref().map(|s| !s.is_empty()).unwrap_or(false) {
                out.push((name.clone(), t!("disk_step.wipe_dialog.target_prefix")));
            }
        }
    }
    if let Some(esp) = &state.disk.esp_partition {
        if let Some(p) = parts.iter().find(|p| &p.name == esp) {
            let is_vfat = p.fstype.as_deref() == Some("vfat");
            if !is_vfat {
                out.push((esp.clone(), t!("disk_step.wipe_dialog.esp_prefix")));
            }
        }
    }
    out
}

/// Build the `(program, args)` pair for invoking `cfdisk /dev/<disk>`. When
/// the installer already runs as root `cfdisk` is spawned directly; otherwise
/// `sudo -- cfdisk <disk>` is used (both `root` and the ISO's `installer` user
/// are passwordless for sudo — see `design.md` §9).
pub(crate) fn cfdisk_command(disk: &str) -> (String, Vec<String>) {
    if is_root() {
        ("cfdisk".to_string(), vec![format!("/dev/{disk}")])
    } else {
        (
            "sudo".to_string(),
            vec![
                "--".to_string(),
                "cfdisk".to_string(),
                format!("/dev/{disk}"),
            ],
        )
    }
}

impl Step for DiskStep {
    fn id(&self) -> StepId {
        StepId::Disk
    }

    fn activate(&mut self, state: &mut InstallerState) {
        self.reset_dialogs();
        // Pick the phase based on shared state: if the user has already
        // assigned roles (e.g. we re-enter via Back from a later step) resume
        // sub-page B; otherwise start on sub-page A.
        let has_assignments =
            state.disk.esp_partition.is_some() || !state.disk.target_partitions.is_empty();
        self.phase = if has_assignments {
            Phase::PartitionAssign
        } else {
            Phase::DiskPicker
        };
        self.refresh_devices();
    }

    fn is_complete(&self, state: &InstallerState) -> bool {
        match self.phase {
            Phase::DiskPicker => true,
            Phase::PartitionAssign => self.assignments_valid(state),
        }
    }

    fn on_subprocess_done(
        &mut self,
        _status: std::process::ExitStatus,
        _state: &mut InstallerState,
    ) {
        self.partprobe();
        self.refresh_devices();
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, state: &InstallerState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        match self.phase {
            Phase::DiskPicker => self.render_disk_picker(frame, chunks[0]),
            Phase::PartitionAssign => self.render_partition_assign(frame, chunks[0], state),
        }

        let hint = match self.phase {
            Phase::DiskPicker => t!("disk_step.hint_disk"),
            Phase::PartitionAssign => t!("disk_step.hint_parts"),
        };
        frame.render_widget(
            Paragraph::new(hint)
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::DIM)),
            chunks[1],
        );

        if self.role_dialog.visible {
            self.render_role_dialog(frame);
        }
        if self.wipe_dialog.visible {
            self.render_wipe_dialog(frame);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> StepAction {
        if key.kind != KeyEventKind::Press {
            return StepAction::None;
        }

        // Wipe dialog swallows all keys except Enter/Esc.
        if self.wipe_dialog.visible {
            match key.code {
                KeyCode::Enter => {
                    self.wipe_dialog.confirmed = true;
                    self.wipe_dialog.visible = false;
                    return StepAction::Next;
                }
                KeyCode::Esc => {
                    self.wipe_dialog.visible = false;
                }
                _ => {}
            }
            return StepAction::None;
        }

        // Role dialog swallows all keys except Up/Down/Enter/Esc.
        if self.role_dialog.visible {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.role_dialog.cursor =
                        (self.role_dialog.cursor + 1) % RoleOption::all().len();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let len = RoleOption::all().len();
                    self.role_dialog.cursor = (self.role_dialog.cursor + len - 1) % len;
                }
                KeyCode::Enter => {
                    let option = RoleOption::all()[self.role_dialog.cursor];
                    self.apply_role(option, state);
                    self.role_dialog.visible = false;
                }
                KeyCode::Esc => {
                    self.role_dialog.visible = false;
                }
                _ => {}
            }
            return StepAction::None;
        }

        match self.phase {
            Phase::DiskPicker => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_highlight(1);
                    StepAction::None
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_highlight(-1);
                    StepAction::None
                }
                KeyCode::Enter => {
                    if let Some(disk) = self.highlighted_disk_name() {
                        let (prog, args) = cfdisk_command(&disk);
                        StepAction::SuspendRun(prog, args)
                    } else {
                        StepAction::None
                    }
                }
                _ => StepAction::None,
            },
            Phase::PartitionAssign => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_highlight(1);
                    StepAction::None
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_highlight(-1);
                    StepAction::None
                }
                KeyCode::Enter => {
                    self.open_role_dialog();
                    StepAction::None
                }
                _ => StepAction::None,
            },
        }
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> bool {
        match self.phase {
            Phase::DiskPicker => {
                self.phase = Phase::PartitionAssign;
                self.list_state.select(Some(0));
                // Mark that we just entered B; is_complete may flip and the
                // Next button visual updates on the next render.
                let _ = state;
                true
            }
            Phase::PartitionAssign => {
                if !self.assignments_valid(state) {
                    return false;
                }
                let wipes = self.compute_wipes(state);
                if !wipes.is_empty() {
                    self.open_wipe_dialog(state);
                    true
                } else {
                    false
                }
            }
        }
    }

    fn on_back_button(&mut self, _state: &mut InstallerState) -> bool {
        match self.phase {
            Phase::PartitionAssign => {
                self.phase = Phase::DiskPicker;
                self.list_state.select(Some(0));
                true
            }
            Phase::DiskPicker => false,
        }
    }
}

impl DiskStep {
    /// Name of the disk under the cursor on sub-page A (e.g. `"sda"`).
    fn highlighted_disk_name(&self) -> Option<String> {
        let idx = self.list_state.selected()?;
        lsblk::flat_disks(&self.disks)
            .get(idx)
            .map(|d| d.name.clone())
    }

    fn render_disk_picker(&mut self, frame: &mut Frame, area: Rect) {
        let disks = lsblk::flat_disks(&self.disks);
        let items: Vec<ListItem> = disks
            .iter()
            .map(|d| {
                let text = format!("{:<10} {:>8}", d.name, lsblk::human_size(d.size));
                ListItem::new(text)
            })
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("disk_step.list_title_disk")),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_partition_assign(&mut self, frame: &mut Frame, area: Rect, state: &InstallerState) {
        let items: Vec<ListItem> = self
            .parts
            .iter()
            .map(|p| {
                let is_esp = state.disk.esp_partition.as_deref() == Some(p.name.as_str());
                let is_target = state.disk.target_partitions.iter().any(|n| n == &p.name);
                let role = if is_esp {
                    t!("disk_step.role_esp")
                } else if is_target {
                    t!("disk_step.role_target")
                } else {
                    "--".to_string()
                };
                let fstype = p.fstype.clone().unwrap_or_else(|| "—".to_string());
                let text = format!(
                    "{:<10} {:>8} {:>8} [{:>6}]",
                    p.name,
                    lsblk::human_size(p.size),
                    fstype,
                    role
                );
                let style = if is_esp || is_target {
                    selected_style()
                } else {
                    Style::default()
                };
                ListItem::new(text).style(style)
            })
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("disk_step.list_title_parts")),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_role_dialog(&self, frame: &mut Frame) {
        let area = centered_rect(50, 9, frame.area());
        let part_name = self.role_dialog.part.clone();
        let title = format!("{}: {}", t!("disk_step.role_dialog.title"), part_name);
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));
        for (i, option) in RoleOption::all().iter().enumerate() {
            let label = match option {
                RoleOption::Esp => t!("disk_step.role_esp"),
                RoleOption::Target => t!("disk_step.role_target"),
                RoleOption::Cancel => t!("disk_step.role_dialog.option_cancel"),
            };
            let prefix = format!("[ {} ]", label);
            let style = if i == self.role_dialog.cursor {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            lines.push(Line::from(ratatui::text::Span::styled(prefix, style)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(t!("disk_step.role_dialog.hint")));
        let dialog = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            .alignment(Alignment::Center);
        frame.render_widget(Clear, area);
        frame.render_widget(dialog, area);
    }

    fn render_wipe_dialog(&self, frame: &mut Frame) {
        let height = (self.wipe_dialog.partitions.len() as u16 + 6).min(frame.area().height);
        let area = centered_rect(70, height, frame.area());
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(t!("disk_step.wipe_dialog.title")));
        lines.push(Line::from(""));
        for (name, reason) in &self.wipe_dialog.partitions {
            lines.push(Line::from(format!("  {reason}  {name}")));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(t!("disk_step.wipe_dialog.hint")));
        let dialog = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("disk_step.wipe_dialog.window_title")),
            )
            .alignment(Alignment::Center);
        frame.render_widget(Clear, area);
        frame.render_widget(dialog, area);
    }
}

/// Centered rect helper for dialogs. `width_pct` is a percentage of
/// `area.width`; `height_rows` is a fixed row count (clamped to area height).
fn centered_rect(width_pct: u16, height_rows: u16, area: Rect) -> Rect {
    let h = height_rows.min(area.height);
    let y = area.y + (area.height - h) / 2;
    let w_pad = (100u16).saturating_sub(width_pct) / 2;
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(w_pad),
            Constraint::Percentage(width_pct),
            Constraint::Percentage(w_pad),
        ])
        .split(area);
    let inner = horizontal[1];
    Rect::new(inner.x, y, inner.width, h)
}

#[cfg(test)]
mod tests;
