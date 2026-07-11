//! Timezone selection step.
//!
//! `timedatectl list-timezones` supplies the available zones. The UI keeps
//! only geographic regions plus the direct UTC choice, excluding legacy
//! top-level aliases and the `Etc` compatibility namespace. GeoIP selects the
//! initial row when possible; unsupported or failed detection falls back to
//! UTC.

use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::geoip;
use crate::util::ui::{focusable_block, rounded_block};
use anyhow::{bail, Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::process::Command;

const REGION_NAMES: [&str; 10] = [
    "Africa",
    "America",
    "Antarctica",
    "Arctic",
    "Asia",
    "Atlantic",
    "Australia",
    "Europe",
    "Indian",
    "Pacific",
];
const UTC: &str = "UTC";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimezoneFocus {
    Region,
    Zone,
}

#[derive(Debug)]
struct TimezoneRegion {
    name: &'static str,
    zones: Vec<String>,
}

/// Two-panel timezone picker used by wizard step 8.
pub struct TimezoneStep {
    regions: Vec<TimezoneRegion>,
    region_state: ListState,
    zone_state: ListState,
    focus: TimezoneFocus,
}

impl TimezoneStep {
    /// Load the timezone database through `timedatectl` and create the picker.
    pub fn new() -> Result<Self> {
        let output = Command::new("timedatectl")
            .arg("list-timezones")
            .output()
            .context("running timedatectl list-timezones")?;
        if !output.status.success() {
            bail!(
                "timedatectl list-timezones failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let text = String::from_utf8(output.stdout)
            .context("timedatectl list-timezones returned non-UTF-8 output")?;
        Self::from_timezone_output(&text)
    }

    fn from_timezone_output(output: &str) -> Result<Self> {
        let regions = parse_timezone_output(output);
        if regions[..REGION_NAMES.len()]
            .iter()
            .any(|region| region.zones.is_empty())
        {
            bail!("timedatectl output is missing a supported geographic region");
        }

        let mut region_state = ListState::default();
        region_state.select(Some(utc_region_index(&regions)));
        let mut zone_state = ListState::default();
        zone_state.select(Some(0));
        Ok(Self {
            regions,
            region_state,
            zone_state,
            focus: TimezoneFocus::Region,
        })
    }

    fn region_index(&self) -> usize {
        self.region_state.selected().unwrap_or(0)
    }

    fn region(&self) -> &TimezoneRegion {
        &self.regions[self.region_index()]
    }

    fn is_utc_region(&self) -> bool {
        self.region().name == UTC
    }

    fn zone_index(&self) -> usize {
        self.zone_state.selected().unwrap_or(0)
    }

    fn highlighted_timezone(&self) -> &str {
        if self.is_utc_region() {
            UTC
        } else {
            &self.region().zones[self.zone_index()]
        }
    }

    fn select_timezone(&mut self, timezone: &str) -> bool {
        if timezone == UTC {
            self.region_state
                .select(Some(utc_region_index(&self.regions)));
            self.zone_state.select(Some(0));
            self.focus = TimezoneFocus::Region;
            return true;
        }

        for (region_index, region) in self.regions.iter().enumerate() {
            if let Some(zone_index) = region.zones.iter().position(|zone| zone == timezone) {
                self.region_state.select(Some(region_index));
                self.zone_state.select(Some(zone_index));
                self.focus = TimezoneFocus::Region;
                return true;
            }
        }
        false
    }

    fn move_region(&mut self, delta: i32) {
        let len = self.regions.len() as i32;
        let next = (self.region_index() as i32 + delta).rem_euclid(len) as usize;
        self.region_state.select(Some(next));
        self.zone_state.select(Some(0));
    }

    fn move_zone(&mut self, delta: i32) {
        let len = self.region().zones.len() as i32;
        if len == 0 {
            return;
        }
        let next = (self.zone_index() as i32 + delta).rem_euclid(len) as usize;
        self.zone_state.select(Some(next));
    }

    fn commit(&self, state: &mut InstallerState) {
        state.timezone = Some(self.highlighted_timezone().to_string());
    }

    fn enter_zone_panel(&mut self) {
        if !self.is_utc_region() {
            self.focus = TimezoneFocus::Zone;
        }
    }

    fn activate_with_detected(&mut self, state: &mut InstallerState, detected: Option<String>) {
        let requested = state.timezone.clone().or(detected);
        let selected = requested
            .filter(|timezone| self.select_timezone(timezone))
            .unwrap_or_else(|| UTC.to_string());
        if selected == UTC {
            self.select_timezone(UTC);
        }
        state.timezone = Some(selected);
    }
}

impl Step for TimezoneStep {
    fn id(&self) -> StepId {
        StepId::Timezone
    }

    fn activate(&mut self, state: &mut InstallerState) -> Result<()> {
        let detected = if state.timezone.is_none() {
            geoip::detect_timezone()?
        } else {
            None
        };
        self.activate_with_detected(state, detected);
        Ok(())
    }

    fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        state: &InstallerState,
        body_focused: bool,
    ) {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(18),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(vertical[0]);

        let selected_timezone = state.timezone.as_deref();
        let region_items = self.regions.iter().map(|region| {
            let selected_region = selected_timezone.is_some_and(|timezone| {
                timezone == UTC && region.name == UTC
                    || timezone.starts_with(&format!("{}/", region.name))
            });
            let style = if selected_region {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(region.name).style(style)
        });
        let region_highlight = if body_focused && self.focus == TimezoneFocus::Region {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let region_list = List::new(region_items)
            .block(focusable_block(
                rounded_block().title(t!("timezone_step.region_title")),
                body_focused && self.focus == TimezoneFocus::Region,
            ))
            .highlight_style(region_highlight);
        frame.render_stateful_widget(region_list, panels[0], &mut self.region_state);

        let zone_disabled = self.is_utc_region();
        let zone_items: Vec<ListItem> = if zone_disabled {
            vec![ListItem::new(t!("timezone_step.utc_direct"))]
        } else {
            self.region()
                .zones
                .iter()
                .map(|zone| {
                    let style = if selected_timezone == Some(zone.as_str()) {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(zone.clone()).style(style)
                })
                .collect()
        };
        let zone_highlight = if body_focused && self.focus == TimezoneFocus::Zone {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let zone_style = if zone_disabled {
            Style::default().add_modifier(Modifier::DIM)
        } else {
            Style::default()
        };
        let zone_list = List::new(zone_items)
            .block(focusable_block(
                rounded_block().title(t!("timezone_step.zone_title")),
                body_focused && self.focus == TimezoneFocus::Zone,
            ))
            .style(zone_style)
            .highlight_style(zone_highlight);
        frame.render_stateful_widget(zone_list, panels[2], &mut self.zone_state);

        let hint = match (self.focus, zone_disabled) {
            (TimezoneFocus::Region, true) => t!("timezone_step.hint_utc"),
            (TimezoneFocus::Region, false) => t!("timezone_step.hint_region"),
            (TimezoneFocus::Zone, _) => t!("timezone_step.hint_zone"),
        };
        frame.render_widget(
            Paragraph::new(hint)
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::DIM)),
            vertical[1],
        );
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }

        Ok(match self.focus {
            TimezoneFocus::Region => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_region(1);
                    StepAction::None
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_region(-1);
                    StepAction::None
                }
                KeyCode::Right => {
                    self.enter_zone_panel();
                    StepAction::None
                }
                KeyCode::Enter if self.is_utc_region() => {
                    self.commit(state);
                    StepAction::Next
                }
                KeyCode::Enter => {
                    self.enter_zone_panel();
                    StepAction::None
                }
                _ => StepAction::None,
            },
            TimezoneFocus::Zone => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_zone(1);
                    StepAction::None
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_zone(-1);
                    StepAction::None
                }
                KeyCode::Left => {
                    self.focus = TimezoneFocus::Region;
                    StepAction::None
                }
                KeyCode::Enter => {
                    self.commit(state);
                    StepAction::Next
                }
                _ => StepAction::None,
            },
        })
    }

    fn consume_tab(&mut self, is_shift: bool) -> bool {
        if self.is_utc_region() {
            self.focus = TimezoneFocus::Region;
            return false;
        }
        match (self.focus, is_shift) {
            (TimezoneFocus::Region, false) => {
                self.focus = TimezoneFocus::Zone;
                true
            }
            (TimezoneFocus::Zone, false) => {
                self.focus = TimezoneFocus::Region;
                false
            }
            (TimezoneFocus::Zone, true) => {
                self.focus = TimezoneFocus::Region;
                true
            }
            (TimezoneFocus::Region, true) => {
                self.focus = TimezoneFocus::Zone;
                false
            }
        }
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        self.commit(state);
        Ok(StepAction::Next)
    }
}

fn parse_timezone_output(output: &str) -> Vec<TimezoneRegion> {
    let mut regions: Vec<TimezoneRegion> = REGION_NAMES
        .iter()
        .map(|name| TimezoneRegion {
            name,
            zones: Vec::new(),
        })
        .collect();

    for timezone in output.lines().map(str::trim) {
        let Some((prefix, _)) = timezone.split_once('/') else {
            continue;
        };
        if let Some(region) = regions.iter_mut().find(|region| region.name == prefix) {
            region.zones.push(timezone.to_string());
        }
    }
    regions.push(TimezoneRegion {
        name: UTC,
        zones: Vec::new(),
    });
    regions
}

fn utc_region_index(regions: &[TimezoneRegion]) -> usize {
    regions
        .iter()
        .position(|region| region.name == UTC)
        .expect("timezone region list always includes UTC")
}

#[cfg(test)]
#[path = "timezone/tests.rs"]
mod tests;
