use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{LineGauge, List, ListItem, ListState, Paragraph, Wrap};

use crate::ui::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallStep {
    Resolving,
    Downloading,
    Unpacking,
    Verifying,
    Linking,
    PostInstall,
}

impl InstallStep {
    pub const ALL: [InstallStep; 6] = [
        InstallStep::Resolving,
        InstallStep::Downloading,
        InstallStep::Unpacking,
        InstallStep::Verifying,
        InstallStep::Linking,
        InstallStep::PostInstall,
    ];

    pub fn label(self) -> &'static str {
        match self {
            InstallStep::Resolving => "Resolving source",
            InstallStep::Downloading => "Downloading",
            InstallStep::Unpacking => "Unpacking",
            InstallStep::Verifying => "Verifying",
            InstallStep::Linking => "Linking",
            InstallStep::PostInstall => "Post-install checks",
        }
    }

    pub fn index(self) -> usize {
        match self {
            InstallStep::Resolving => 0,
            InstallStep::Downloading => 1,
            InstallStep::Unpacking => 2,
            InstallStep::Verifying => 3,
            InstallStep::Linking => 4,
            InstallStep::PostInstall => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillState {
    Queued,
    Running,
    Done,
    Failed,
}

#[derive(Clone, Debug)]
pub struct InstallSkill {
    pub name: String,
    pub version: Option<String>,
    pub requested: String,
    pub source: String,
    pub commit: String,
    pub state: SkillState,
    pub current_step: Option<InstallStep>,
    pub step_details: Vec<Option<String>>,
    pub error: Option<String>,
}

impl InstallSkill {
    pub fn from_queue(entry: QueuedSkill) -> Self {
        Self {
            name: entry.name,
            version: entry.version,
            requested: entry.requested,
            source: entry.source,
            commit: entry.commit,
            state: SkillState::Queued,
            current_step: None,
            step_details: vec![None; InstallStep::ALL.len()],
            error: None,
        }
    }

    pub fn progress(&self) -> f64 {
        match self.state {
            SkillState::Queued => 0.0,
            SkillState::Running => self
                .current_step
                .map(|step| (step.index() + 1) as f64 / InstallStep::ALL.len() as f64)
                .unwrap_or(0.05),
            SkillState::Done => 1.0,
            SkillState::Failed => self
                .current_step
                .map(|step| (step.index() + 1) as f64 / InstallStep::ALL.len() as f64)
                .unwrap_or(0.0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct QueuedSkill {
    pub name: String,
    pub version: Option<String>,
    pub requested: String,
    pub source: String,
    pub commit: String,
}

#[derive(Clone, Debug, Default)]
pub struct InstallViewModel {
    pub context: String,
    pub skills: Vec<InstallSkill>,
    pub view: Vec<usize>,
    pub list_state: ListState,
    pub active_skill: Option<usize>,
}

pub struct InstallView;

impl InstallView {
    pub fn render(frame: &mut ratatui::Frame, area: Rect, model: &InstallViewModel) {
        let layout = RenderLayout::from_area(area);
        if layout.header.height > 0 {
            render_header(frame, layout.header, model);
        }
        if let Some(gauge) = layout.gauge {
            render_gauge(frame, gauge, model);
        }
        if layout.list.height > 0 {
            render_list(frame, layout.list, model);
        }
    }
}

pub fn list_height(area: Rect) -> u16 {
    RenderLayout::from_area(area).list_height
}

fn render_header(frame: &mut ratatui::Frame, area: Rect, model: &InstallViewModel) {
    let ratio = aggregate_progress(model);
    let percent = format!("{:.0}%", ratio * 100.0);
    let mut spans = vec![
        Span::from("Installing skills... "),
        Span::from(format!("({percent})"))
            .style(theme::accent_style())
            .bold(),
    ];
    if !model.context.is_empty() {
        spans.push("  ".into());
        spans.push(Span::from(model.context.clone()).dim());
    }
    let header = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
    frame.render_widget(header, area);
}

fn render_gauge(frame: &mut ratatui::Frame, area: Rect, model: &InstallViewModel) {
    let ratio = aggregate_progress(model);
    let label = format!("{:.0}%", ratio * 100.0);
    let gauge = LineGauge::default()
        .ratio(ratio)
        .label(label)
        .filled_style(theme::accent_style());
    frame.render_widget(gauge, area);
}

fn render_list(frame: &mut ratatui::Frame, area: Rect, model: &InstallViewModel) {
    if model.view.is_empty() {
        let placeholder = Paragraph::new("Resolving install queue...")
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });
        frame.render_widget(placeholder, area);
        return;
    }

    let items = build_skill_items(model, area.width);
    let list = List::new(items);
    let mut state = model.list_state;
    frame.render_stateful_widget(list, area, &mut state);
}

fn build_skill_items(model: &InstallViewModel, width: u16) -> Vec<ListItem<'_>> {
    let expanded = expanded_skill_index(model);
    let mut items = Vec::with_capacity(model.view.len());

    for &index in &model.view {
        let Some(skill) = model.skills.get(index) else {
            continue;
        };
        if Some(index) == expanded {
            items.push(ListItem::new(expanded_lines(skill, width)));
        } else {
            items.push(ListItem::new(vec![collapsed_line(skill)]));
        }
    }

    items
}

fn collapsed_line(skill: &InstallSkill) -> Line<'_> {
    let status = skill_status_span(skill);
    let name = match skill.state {
        SkillState::Failed => Span::from(skill.name.clone()).red(),
        _ => Span::from(skill.name.clone()).dim(),
    };
    let mut spans = vec![status, " ".into(), name];
    if let Some(label) = version_label(skill) {
        spans.push(" ".into());
        spans.push(Span::from(label).dim());
    }
    Line::from(spans)
}

fn expanded_lines(skill: &InstallSkill, width: u16) -> Vec<Line<'_>> {
    let separator = separator_line(width);
    let mut lines = Vec::new();

    let summary_status = skill_status_span(skill);
    let summary_name = Span::from(skill.name.clone())
        .style(theme::accent_style())
        .bold();
    let summary_version = version_label(skill).map(|label| Span::from(label).dim());
    let status_text = match skill.state {
        SkillState::Running => Span::from("Installing...").dim(),
        SkillState::Failed => Span::from("Failed").red(),
        SkillState::Done => Span::from("Installed").green(),
        SkillState::Queued => Span::from("Queued").dim(),
    };
    let mut summary = vec![summary_status, " ".into(), summary_name];
    if let Some(version) = summary_version {
        summary.push(" ".into());
        summary.push(version);
    }
    summary.push("  ".into());
    summary.push(status_text);
    lines.push(Line::from(summary));

    lines.push(separator.clone());
    lines.push(Line::from(""));

    let percent = format!("{:.0}%", skill.progress() * 100.0);
    let mut progress = vec![
        Span::from(skill.name.clone())
            .style(theme::accent_style())
            .bold(),
    ];
    if let Some(label) = version_label(skill) {
        progress.push(" ".into());
        progress.push(Span::from(label).dim());
    }
    progress.push("  ".into());
    progress.push(Span::from(format!("({percent})")).style(theme::accent_style()));
    lines.push(Line::from(progress));

    if !skill.source.is_empty() || !skill.commit.is_empty() {
        let mut meta = vec![Span::from("  source: ").dim()];
        if !skill.source.is_empty() {
            meta.push(Span::from(skill.source.clone()).dim());
        } else {
            meta.push(Span::from("unknown").dim());
        }
        if !skill.commit.is_empty() {
            meta.push(Span::from("  commit: ").dim());
            meta.push(Span::from(short_commit(&skill.commit)).dim());
        }
        lines.push(Line::from(meta));
    }

    if let Some(error) = &skill.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::from(format!("  Error: {error}")).red()));
    }

    lines.push(Line::from(""));

    for (idx, step) in InstallStep::ALL.iter().enumerate() {
        let status = step_status(skill, *step);
        let label = match status {
            StepStatus::Active => Span::from(step.label()).style(theme::accent_style()).bold(),
            StepStatus::Done => Span::from(step.label()).style(theme::success_style()).dim(),
            StepStatus::Pending => Span::from(step.label()).dim(),
        };
        lines.push(Line::from(vec!["  ".into(), label]));

        let detail = skill
            .step_details
            .get(step.index())
            .and_then(|value| value.clone());
        match (detail, status) {
            (Some(detail), StepStatus::Active) => {
                lines.push(Line::from(vec![
                    "    -> ".into(),
                    Span::from(trim_detail(&detail)).style(theme::accent_style()),
                ]));
            }
            (Some(detail), _) => {
                lines.push(Line::from(vec![
                    "    -> ".into(),
                    Span::from(trim_detail(&detail)).dim(),
                ]));
            }
            (None, StepStatus::Pending) => {
                lines.push(Line::from(vec![
                    "    -> ".into(),
                    Span::from("Waiting...").dim(),
                ]));
            }
            _ => {}
        }

        if idx + 1 < InstallStep::ALL.len() {
            lines.push(Line::from(""));
        }
    }

    lines.push(Line::from(""));
    lines.push(separator);
    lines
}

fn skill_status_span(skill: &InstallSkill) -> Span<'static> {
    match skill.state {
        SkillState::Queued => "[ ]".dim(),
        SkillState::Running => Span::styled("[>]", theme::accent_style()),
        SkillState::Done => Span::styled("[✓]", theme::success_style()),
        SkillState::Failed => Span::styled("[✗]", theme::error_style()),
    }
}

fn step_status(skill: &InstallSkill, step: InstallStep) -> StepStatus {
    let current = skill.current_step;
    match skill.state {
        SkillState::Done => StepStatus::Done,
        SkillState::Failed => match current {
            Some(curr) if curr == step => StepStatus::Active,
            Some(curr) if curr.index() >= step.index() => StepStatus::Done,
            _ => StepStatus::Pending,
        },
        _ => match current {
            Some(curr) if curr == step => StepStatus::Active,
            Some(curr) if curr.index() > step.index() => StepStatus::Done,
            _ => StepStatus::Pending,
        },
    }
}

fn expanded_skill_index(model: &InstallViewModel) -> Option<usize> {
    let index = model.active_skill?;
    model.skills.get(index).and_then(|skill| {
        if matches!(skill.state, SkillState::Running | SkillState::Failed) {
            Some(index)
        } else {
            None
        }
    })
}

fn separator_line(width: u16) -> Line<'static> {
    let count = width.max(10) as usize;
    Line::from("-".repeat(count))
}

fn version_label(skill: &InstallSkill) -> Option<String> {
    if let Some(version) = &skill.version {
        return Some(format!("@{version}"));
    }
    if skill.requested.trim().is_empty() {
        return None;
    }
    Some(skill.requested.clone())
}

fn aggregate_progress(model: &InstallViewModel) -> f64 {
    if model.skills.is_empty() {
        return 0.0;
    }
    let total: f64 = model.skills.iter().map(|skill| skill.progress()).sum();
    total / model.skills.len() as f64
}

fn trim_detail(value: &str) -> String {
    let trimmed = value.trim();
    const LIMIT: usize = 48;
    if trimmed.len() <= LIMIT {
        return trimmed.to_string();
    }
    let mut out = trimmed[..LIMIT].to_string();
    out.push_str("...");
    out
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(7).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StepStatus {
    Pending,
    Active,
    Done,
}

#[derive(Clone, Copy)]
struct RenderLayout {
    header: Rect,
    gauge: Option<Rect>,
    list: Rect,
    list_height: u16,
}

impl RenderLayout {
    fn from_area(area: Rect) -> Self {
        let (header, gauge, list) = if area.height >= 3 {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Min(1),
                    ]
                    .as_ref(),
                )
                .split(area);
            (chunks[0], Some(chunks[1]), chunks[2])
        } else if area.height == 2 {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)].as_ref())
                .split(area);
            (chunks[0], None, chunks[1])
        } else {
            (
                area,
                None,
                Rect::new(area.x, area.y.saturating_add(1), area.width, 0),
            )
        };

        let list_height = list.height;

        Self {
            header,
            gauge,
            list,
            list_height,
        }
    }
}
