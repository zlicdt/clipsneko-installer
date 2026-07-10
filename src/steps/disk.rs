//! Disk partitioning: cfdisk launcher, partition roles, RAID profile, and
//! precise destructive-operation confirmation.

use crate::state::{BtrfsRaidMode, InstallerState};
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::lsblk::{self, BlockDevice};
use crate::util::process::{is_root, privileged_command};
use crate::util::ui::centered_rect;
use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;

fn selected_style() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    DiskPicker,
    PartitionAssign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoleOption {
    Esp,
    Target,
    Unassigned,
}

impl RoleOption {
    const fn all() -> [Self; 3] {
        [Self::Esp, Self::Target, Self::Unassigned]
    }
}

#[derive(Debug)]
struct RoleDialog {
    visible: bool,
    cursor: usize,
    part: String,
}

#[derive(Debug, Default)]
struct RaidDialog {
    visible: bool,
    cursor: usize,
}

#[derive(Debug, Default)]
struct ErrorDialog {
    visible: bool,
    message: String,
}

#[derive(Debug, Default)]
struct WipeDialog {
    visible: bool,
    partitions: Vec<(String, String)>,
}

pub struct DiskStep {
    phase: Phase,
    disks: Vec<BlockDevice>,
    parts: Vec<BlockDevice>,
    table_state: TableState,
    role_dialog: RoleDialog,
    raid_dialog: RaidDialog,
    error_dialog: ErrorDialog,
    wipe_dialog: WipeDialog,
}

impl DiskStep {
    pub fn new() -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        Self {
            phase: Phase::DiskPicker,
            disks: Vec::new(),
            parts: Vec::new(),
            table_state,
            role_dialog: RoleDialog {
                visible: false,
                cursor: 0,
                part: String::new(),
            },
            raid_dialog: RaidDialog::default(),
            error_dialog: ErrorDialog::default(),
            wipe_dialog: WipeDialog::default(),
        }
    }

    fn refresh_devices(&mut self, state: &mut InstallerState) -> Result<()> {
        self.disks = lsblk::list_devices()?;
        self.parts = lsblk::flat_parts(&self.disks)
            .into_iter()
            .cloned()
            .collect();
        let protected = protected_partition_names(&self.disks);
        reconcile_assignments(&self.parts, &protected, state);
        self.clamp_cursor();
        tracing::debug!(
            "lsblk refresh: {} disk candidates, {} partitions",
            lsblk::flat_disks(&self.disks).len(),
            self.parts.len()
        );
        Ok(())
    }

    fn partprobe(&mut self) -> Result<bool> {
        let output = privileged_command("partprobe")
            .output()
            .context("running partprobe")?;
        if output.status.success() {
            return Ok(true);
        }
        tracing::warn!(
            "partprobe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        self.show_error(t!("disk_step.error_partprobe"));
        Ok(false)
    }

    fn current_list_len(&self) -> usize {
        match self.phase {
            Phase::DiskPicker => lsblk::flat_disks(&self.disks).len(),
            Phase::PartitionAssign => self.parts.len(),
        }
    }

    fn clamp_cursor(&mut self) {
        let len = self.current_list_len();
        if len == 0 {
            self.table_state.select(None);
        } else {
            let current = self.table_state.selected().unwrap_or(0);
            self.table_state.select(Some(current.min(len - 1)));
        }
    }

    fn move_highlight(&mut self, delta: i32) {
        let len = self.current_list_len() as i32;
        if len == 0 {
            return;
        }
        let current = self.table_state.selected().unwrap_or(0) as i32;
        self.table_state
            .select(Some((current + delta).rem_euclid(len) as usize));
    }

    fn reset_dialogs(&mut self) {
        self.role_dialog.visible = false;
        self.raid_dialog.visible = false;
        self.error_dialog.visible = false;
        self.wipe_dialog.visible = false;
    }

    fn roles_valid(&self, state: &InstallerState) -> bool {
        let Some(esp_name) = state.disk.esp_partition.as_deref() else {
            return false;
        };
        let Some(esp) = self.parts.iter().find(|part| part.name == esp_name) else {
            return false;
        };
        let protected = protected_partition_names(&self.disks);
        if !lsblk::is_esp_partition(esp)
            || protected.iter().any(|name| name == esp_name)
            || state.disk.target_partitions.is_empty()
        {
            return false;
        }
        state.disk.target_partitions.iter().all(|target| {
            target != esp_name
                && !protected.iter().any(|name| name == target)
                && self.parts.iter().any(|part| &part.name == target)
        })
    }

    fn usable_target_bytes(&self, state: &InstallerState) -> Option<u64> {
        let sizes: Vec<u64> = state
            .disk
            .target_partitions
            .iter()
            .map(|name| {
                self.parts
                    .iter()
                    .find(|part| &part.name == name)
                    .map(|part| part.size)
            })
            .collect::<Option<Vec<_>>>()?;
        usable_capacity(&sizes, state.disk.raid_mode)
    }

    fn highlighted_disk(&self) -> Option<&BlockDevice> {
        let index = self.table_state.selected()?;
        lsblk::flat_disks(&self.disks).get(index).copied()
    }

    fn highlighted_disk_name(&self) -> Option<String> {
        self.highlighted_disk()
            .filter(|disk| disk_is_selectable(disk))
            .map(|disk| disk.name.clone())
    }

    fn highlighted_part_name(&self) -> Option<String> {
        let index = self.table_state.selected()?;
        self.parts.get(index).map(|part| part.name.clone())
    }

    fn open_role_dialog(&mut self, state: &InstallerState) {
        let Some(part) = self.highlighted_part_name() else {
            return;
        };
        if protected_partition_names(&self.disks)
            .iter()
            .any(|name| name == &part)
        {
            self.show_error(t!("disk_step.error_protected_part"));
            return;
        }
        let cursor = if state.disk.esp_partition.as_deref() == Some(part.as_str()) {
            0
        } else if state
            .disk
            .target_partitions
            .iter()
            .any(|name| name == &part)
        {
            1
        } else {
            2
        };
        self.role_dialog = RoleDialog {
            visible: true,
            cursor,
            part,
        };
    }

    fn apply_role(&mut self, option: RoleOption, state: &mut InstallerState) {
        let part = self.role_dialog.part.clone();
        match option {
            RoleOption::Esp => {
                let valid_esp = self
                    .parts
                    .iter()
                    .find(|candidate| candidate.name == part)
                    .is_some_and(lsblk::is_esp_partition);
                if !valid_esp {
                    self.show_error(t!("disk_step.error_invalid_esp"));
                    return;
                }
                state.disk.target_partitions.retain(|name| name != &part);
                state.disk.esp_partition = Some(part);
            }
            RoleOption::Target => {
                if state.disk.esp_partition.as_deref() == Some(part.as_str()) {
                    state.disk.esp_partition = None;
                }
                if !state
                    .disk
                    .target_partitions
                    .iter()
                    .any(|name| name == &part)
                {
                    state.disk.target_partitions.push(part);
                }
            }
            RoleOption::Unassigned => {
                if state.disk.esp_partition.as_deref() == Some(part.as_str()) {
                    state.disk.esp_partition = None;
                }
                state.disk.target_partitions.retain(|name| name != &part);
            }
        }
        state.disk.raid_mode = None;
    }

    fn open_raid_dialog(&mut self, state: &InstallerState) {
        self.raid_dialog.visible = true;
        self.raid_dialog.cursor = match state.disk.raid_mode {
            Some(BtrfsRaidMode::Raid1) => 1,
            _ => 0,
        };
    }

    fn show_error(&mut self, message: String) {
        self.error_dialog = ErrorDialog {
            visible: true,
            message,
        };
    }

    fn try_advance(&mut self, state: &mut InstallerState) -> StepAction {
        if !self.roles_valid(state) {
            return StepAction::None;
        }
        if state.disk.target_partitions.len() > 1 && state.disk.raid_mode.is_none() {
            self.open_raid_dialog(state);
            return StepAction::None;
        }
        if self
            .usable_target_bytes(state)
            .is_none_or(|bytes| bytes <= lsblk::TARGET_MIN_BYTES)
        {
            state.disk.raid_mode = None;
            self.show_error(t!("disk_step.error_capacity"));
            return StepAction::None;
        }
        let wipes = compute_wipe_list(&self.parts, state);
        if wipes.is_empty() {
            StepAction::Next
        } else {
            self.wipe_dialog = WipeDialog {
                visible: true,
                partitions: wipes,
            };
            StepAction::None
        }
    }

    fn render_disk_picker(&mut self, frame: &mut Frame, area: Rect, body_focused: bool) {
        let rows = lsblk::flat_disks(&self.disks).into_iter().map(|disk| {
            let style = if disk_is_selectable(disk) {
                Style::default()
            } else {
                Style::default().add_modifier(Modifier::DIM)
            };
            Row::new(vec![
                Cell::from(disk.name.clone()),
                Cell::from(optional_text(&disk.model)),
                Cell::from(optional_text(&disk.tran)),
                Cell::from(lsblk::human_size(disk.size)),
                Cell::from(disk_status(disk)),
            ])
            .style(style)
        });
        let header = Row::new(vec![
            t!("disk_step.column_device"),
            t!("disk_step.column_model"),
            t!("disk_step.column_transport"),
            t!("disk_step.column_size"),
            t!("disk_step.column_status"),
        ])
        .style(Style::default().add_modifier(Modifier::BOLD));
        let row_highlight = if body_focused {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let table = Table::new(
            rows,
            [
                Constraint::Length(13),
                Constraint::Min(10),
                Constraint::Length(9),
                Constraint::Length(9),
                Constraint::Length(11),
            ],
        )
        .header(header)
        .column_spacing(1)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(t!("disk_step.list_title_disk")),
        )
        .row_highlight_style(row_highlight);
        frame.render_stateful_widget(table, area, &mut self.table_state);
    }

    fn render_partition_assign(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        state: &InstallerState,
        body_focused: bool,
    ) {
        let rows = self.parts.iter().map(|part| {
            let protected = protected_partition_names(&self.disks)
                .iter()
                .any(|name| name == &part.name);
            let is_esp = state.disk.esp_partition.as_deref() == Some(part.name.as_str());
            let is_target = state
                .disk
                .target_partitions
                .iter()
                .any(|name| name == &part.name);
            let role = if protected {
                t!("disk_step.role_protected")
            } else if is_esp {
                t!("disk_step.role_esp")
            } else if is_target {
                t!("disk_step.role_target")
            } else {
                t!("disk_step.role_unassigned")
            };
            let style = if protected {
                Style::default().add_modifier(Modifier::DIM)
            } else if is_esp || is_target {
                selected_style()
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(part.name.clone()),
                Cell::from(lsblk::human_size(part.size)),
                Cell::from(optional_text(&part.fstype)),
                Cell::from(optional_text(&part.partlabel)),
                Cell::from(role),
            ])
            .style(style)
        });
        let header = Row::new(vec![
            t!("disk_step.column_device"),
            t!("disk_step.column_size"),
            t!("disk_step.column_filesystem"),
            t!("disk_step.column_label"),
            t!("disk_step.column_role"),
        ])
        .style(Style::default().add_modifier(Modifier::BOLD));
        let row_highlight = if body_focused {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let table = Table::new(
            rows,
            [
                Constraint::Length(13),
                Constraint::Length(8),
                Constraint::Length(9),
                Constraint::Min(8),
                Constraint::Length(10),
            ],
        )
        .header(header)
        .column_spacing(1)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(t!("disk_step.list_title_parts")),
        )
        .row_highlight_style(row_highlight);
        frame.render_stateful_widget(table, area, &mut self.table_state);
    }

    fn render_role_dialog(&self, frame: &mut Frame, state: &InstallerState) {
        let area = centered_rect(80, 9, frame.area());
        let title = format!(
            "{}: {}",
            t!("disk_step.role_dialog.title"),
            self.role_dialog.part
        );
        let current = current_role(&self.role_dialog.part, state);
        let mut lines = vec![Line::from("")];
        for (index, option) in RoleOption::all().iter().enumerate() {
            let style = if index == self.role_dialog.cursor {
                Style::default().add_modifier(Modifier::REVERSED)
            } else if Some(*option) == current {
                selected_style()
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(
                format!("[ {} ]", role_label(*option)),
                style,
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(t!("disk_step.role_dialog.hint")));
        let dialog = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            .alignment(Alignment::Center);
        frame.render_widget(Clear, area);
        frame.render_widget(dialog, area);
    }

    fn render_raid_dialog(&self, frame: &mut Frame) {
        let area = centered_rect(70, 7, frame.area());
        let options = [
            t!("disk_step.raid_dialog.raid0"),
            t!("disk_step.raid_dialog.raid1"),
        ];
        let mut lines = vec![Line::from("")];
        for (index, label) in options.into_iter().enumerate() {
            let style = if index == self.raid_dialog.cursor {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(format!("[ {label} ]"), style)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(t!("disk_step.raid_dialog.hint")));
        let dialog = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("disk_step.raid_dialog.title")),
            )
            .alignment(Alignment::Center);
        frame.render_widget(Clear, area);
        frame.render_widget(dialog, area);
    }

    fn render_error_dialog(&self, frame: &mut Frame) {
        let area = centered_rect(80, 7, frame.area());
        let dialog = Paragraph::new(vec![
            Line::from(""),
            Line::from(self.error_dialog.message.clone()),
            Line::from(""),
            Line::from(t!("disk_step.error_hint")),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(t!("disk_step.error_title")),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        frame.render_widget(Clear, area);
        frame.render_widget(dialog, area);
    }

    fn render_wipe_dialog(&self, frame: &mut Frame) {
        let height = (self.wipe_dialog.partitions.len() as u16 * 2 + 6).min(frame.area().height);
        let area = centered_rect(80, height, frame.area());
        let mut lines = vec![
            Line::from(t!("disk_step.wipe_dialog.title")),
            Line::from(""),
        ];
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
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(Clear, area);
        frame.render_widget(dialog, area);
    }
}

fn optional_text(value: &Option<String>) -> String {
    value
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or("—")
        .to_string()
}

fn disk_is_selectable(disk: &BlockDevice) -> bool {
    !disk.ro && !lsblk::is_live_media_disk(disk)
}

fn disk_status(disk: &BlockDevice) -> String {
    if lsblk::is_live_media_disk(disk) {
        t!("disk_step.status_live_media")
    } else if disk.ro {
        t!("disk_step.status_read_only")
    } else if disk.rm {
        t!("disk_step.status_removable")
    } else {
        t!("disk_step.status_available")
    }
}

fn role_label(option: RoleOption) -> String {
    match option {
        RoleOption::Esp => t!("disk_step.role_esp"),
        RoleOption::Target => t!("disk_step.role_target"),
        RoleOption::Unassigned => t!("disk_step.role_unassigned"),
    }
}

fn current_role(part: &str, state: &InstallerState) -> Option<RoleOption> {
    if state.disk.esp_partition.as_deref() == Some(part) {
        Some(RoleOption::Esp)
    } else if state.disk.target_partitions.iter().any(|name| name == part) {
        Some(RoleOption::Target)
    } else {
        Some(RoleOption::Unassigned)
    }
}

/// Conservative usable-capacity estimate for the selected btrfs data profile.
/// RAID0 is limited by the smallest device because all devices are striped;
/// RAID1 is limited both by two-copy overhead and by space outside the largest
/// device that can hold its second copy.
fn usable_capacity(sizes: &[u64], raid_mode: Option<BtrfsRaidMode>) -> Option<u64> {
    match sizes {
        [] => None,
        [single] => Some(*single),
        multiple => match raid_mode? {
            BtrfsRaidMode::Raid0 => multiple
                .iter()
                .min()
                .and_then(|smallest| smallest.checked_mul(multiple.len() as u64)),
            BtrfsRaidMode::Raid1 => {
                let total = multiple
                    .iter()
                    .try_fold(0_u64, |sum, size| sum.checked_add(*size))?;
                let largest = *multiple.iter().max()?;
                Some((total / 2).min(total - largest))
            }
        },
    }
}

/// Return every partition that the install stage will format. Every Target is
/// included regardless of detected FSTYPE; an existing vfat ESP is reused.
pub(crate) fn compute_wipe_list(
    parts: &[BlockDevice],
    state: &InstallerState,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for name in &state.disk.target_partitions {
        if parts.iter().any(|part| &part.name == name) {
            out.push((name.clone(), t!("disk_step.wipe_dialog.target_prefix")));
        }
    }
    if let Some(esp) = &state.disk.esp_partition {
        if let Some(part) = parts.iter().find(|part| &part.name == esp) {
            if part.fstype.as_deref() != Some("vfat") {
                out.push((esp.clone(), t!("disk_step.wipe_dialog.esp_prefix")));
            }
        }
    }
    out
}

fn protected_partition_names(disks: &[BlockDevice]) -> Vec<String> {
    fn collect(device: &BlockDevice, names: &mut Vec<String>) {
        if device.kind == "part" {
            names.push(device.name.clone());
        }
        if let Some(children) = device.children.as_deref() {
            for child in children {
                collect(child, names);
            }
        }
    }

    let mut names = Vec::new();
    for disk in lsblk::flat_disks(disks) {
        if !disk_is_selectable(disk) {
            collect(disk, &mut names);
        }
    }
    names
}

fn reconcile_assignments(parts: &[BlockDevice], protected: &[String], state: &mut InstallerState) {
    let previous_targets = state.disk.target_partitions.clone();
    let previous_esp = state.disk.esp_partition.clone();
    let exists = |name: &str| parts.iter().any(|part| part.name == name);

    if state
        .disk
        .esp_partition
        .as_deref()
        .is_some_and(|esp| !exists(esp) || protected.iter().any(|name| name == esp))
    {
        state.disk.esp_partition = None;
    }
    state
        .disk
        .target_partitions
        .retain(|target| exists(target) && !protected.iter().any(|name| name == target));
    if let Some(esp) = &state.disk.esp_partition {
        state.disk.target_partitions.retain(|target| target != esp);
    }
    if state.disk.target_partitions != previous_targets || state.disk.esp_partition != previous_esp
    {
        state.disk.raid_mode = None;
    }
}

/// Build the full-screen cfdisk command for a device name.
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

    fn activate(&mut self, state: &mut InstallerState) -> Result<()> {
        self.reset_dialogs();
        let has_assignments =
            state.disk.esp_partition.is_some() || !state.disk.target_partitions.is_empty();
        self.phase = if has_assignments {
            Phase::PartitionAssign
        } else {
            Phase::DiskPicker
        };
        self.refresh_devices(state)
    }

    fn is_complete(&self, state: &InstallerState) -> bool {
        match self.phase {
            Phase::DiskPicker => lsblk::flat_disks(&self.disks)
                .into_iter()
                .any(disk_is_selectable),
            Phase::PartitionAssign => self.roles_valid(state),
        }
    }

    fn has_modal(&self) -> bool {
        self.role_dialog.visible
            || self.raid_dialog.visible
            || self.error_dialog.visible
            || self.wipe_dialog.visible
    }

    fn on_subprocess_done(
        &mut self,
        _status: std::process::ExitStatus,
        state: &mut InstallerState,
    ) -> Result<()> {
        state.disk = Default::default();
        // Never expose the pre-cfdisk partition snapshot after partprobe has
        // reported that the kernel could not refresh the partition table.
        self.parts.clear();
        if self.partprobe()? {
            self.refresh_devices(state)?;
        }
        Ok(())
    }

    fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        state: &InstallerState,
        body_focused: bool,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        match self.phase {
            Phase::DiskPicker => self.render_disk_picker(frame, chunks[0], body_focused),
            Phase::PartitionAssign => {
                self.render_partition_assign(frame, chunks[0], state, body_focused)
            }
        }
        let mut hint = match self.phase {
            Phase::DiskPicker => t!("disk_step.hint_disk"),
            Phase::PartitionAssign => t!("disk_step.hint_parts"),
        };
        if self.phase == Phase::PartitionAssign && state.disk.target_partitions.len() > 1 {
            let raid = match state.disk.raid_mode {
                Some(BtrfsRaidMode::Raid0) => "RAID0",
                Some(BtrfsRaidMode::Raid1) => "RAID1",
                None => "—",
            };
            hint = format!("{hint}  {}: {raid}", t!("disk_step.raid_label"));
        }
        frame.render_widget(
            Paragraph::new(hint)
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::DIM)),
            chunks[1],
        );

        if self.role_dialog.visible {
            self.render_role_dialog(frame, state);
        }
        if self.raid_dialog.visible {
            self.render_raid_dialog(frame);
        }
        if self.error_dialog.visible {
            self.render_error_dialog(frame);
        }
        if self.wipe_dialog.visible {
            self.render_wipe_dialog(frame);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }
        if self.error_dialog.visible {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                self.error_dialog.visible = false;
            }
            return Ok(StepAction::None);
        }
        if self.wipe_dialog.visible {
            return Ok(match key.code {
                KeyCode::Enter => {
                    self.wipe_dialog.visible = false;
                    StepAction::Next
                }
                KeyCode::Esc => {
                    self.wipe_dialog.visible = false;
                    StepAction::None
                }
                _ => StepAction::None,
            });
        }
        if self.raid_dialog.visible {
            return Ok(match key.code {
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Up | KeyCode::Char('k') => {
                    self.raid_dialog.cursor = 1 - self.raid_dialog.cursor;
                    StepAction::None
                }
                KeyCode::Enter => {
                    state.disk.raid_mode = Some(if self.raid_dialog.cursor == 0 {
                        BtrfsRaidMode::Raid0
                    } else {
                        BtrfsRaidMode::Raid1
                    });
                    self.raid_dialog.visible = false;
                    self.try_advance(state)
                }
                KeyCode::Esc => {
                    self.raid_dialog.visible = false;
                    StepAction::None
                }
                _ => StepAction::None,
            });
        }
        if self.role_dialog.visible {
            return Ok(match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.role_dialog.cursor =
                        (self.role_dialog.cursor + 1) % RoleOption::all().len();
                    StepAction::None
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let len = RoleOption::all().len();
                    self.role_dialog.cursor = (self.role_dialog.cursor + len - 1) % len;
                    StepAction::None
                }
                KeyCode::Enter => {
                    let option = RoleOption::all()[self.role_dialog.cursor];
                    self.role_dialog.visible = false;
                    self.apply_role(option, state);
                    StepAction::None
                }
                KeyCode::Esc => {
                    self.role_dialog.visible = false;
                    StepAction::None
                }
                _ => StepAction::None,
            });
        }

        Ok(match self.phase {
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
                        let (program, args) = cfdisk_command(&disk);
                        StepAction::SuspendRun(program, args)
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
                    self.open_role_dialog(state);
                    StepAction::None
                }
                KeyCode::Char('r') | KeyCode::Char('R')
                    if state.disk.target_partitions.len() > 1 =>
                {
                    self.open_raid_dialog(state);
                    StepAction::None
                }
                _ => StepAction::None,
            },
        })
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        Ok(match self.phase {
            Phase::DiskPicker => {
                self.phase = Phase::PartitionAssign;
                self.table_state.select(Some(0));
                self.clamp_cursor();
                StepAction::None
            }
            Phase::PartitionAssign => self.try_advance(state),
        })
    }

    fn on_back_button(&mut self, _state: &mut InstallerState) -> Result<StepAction> {
        Ok(match self.phase {
            Phase::PartitionAssign => {
                self.phase = Phase::DiskPicker;
                self.table_state.select(Some(0));
                self.clamp_cursor();
                StepAction::None
            }
            Phase::DiskPicker => StepAction::Back,
        })
    }
}

#[cfg(test)]
mod tests;
