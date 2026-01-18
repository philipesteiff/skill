use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, LineGauge, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui::{Terminal, TerminalOptions, Viewport};
use std::io::{self, Stdout};
use std::time::Duration;

pub struct QueuedSkill {
    pub name: String,
    pub version: Option<String>,
    pub requested: String,
    pub source: String,
    pub commit: String,
}

pub struct SkillUpdate {
    pub name: Option<String>,
    pub version: Option<String>,
}

pub enum Reporter {
    Console,
    Tui(TuiReporter),
}

impl Reporter {
    pub fn new(use_tui: bool) -> Result<Self> {
        if use_tui {
            Ok(Reporter::Tui(TuiReporter::new()?))
        } else {
            Ok(Reporter::Console)
        }
    }

    pub fn set_context(&mut self, context: impl Into<String>) -> Result<()> {
        match self {
            Reporter::Console => Ok(()),
            Reporter::Tui(reporter) => reporter.set_context(context.into()),
        }
    }

    pub fn queue_skills(&mut self, skills: Vec<QueuedSkill>) -> Result<()> {
        match self {
            Reporter::Console => Ok(()),
            Reporter::Tui(reporter) => reporter.queue_skills(skills),
        }
    }

    pub fn begin_skill(&mut self, index: usize) -> Result<()> {
        match self {
            Reporter::Console => Ok(()),
            Reporter::Tui(reporter) => reporter.begin_skill(index),
        }
    }

    pub fn update_active_skill(&mut self, update: SkillUpdate) -> Result<()> {
        match self {
            Reporter::Console => Ok(()),
            Reporter::Tui(reporter) => reporter.update_active_skill(update),
        }
    }

    pub fn finish_skill(&mut self) -> Result<()> {
        match self {
            Reporter::Console => Ok(()),
            Reporter::Tui(reporter) => reporter.finish_skill(),
        }
    }

    pub fn fail_active_skill(&mut self, error: impl Into<String>) -> Result<()> {
        match self {
            Reporter::Console => Ok(()),
            Reporter::Tui(reporter) => reporter.fail_active_skill(error.into()),
        }
    }

    pub fn step(&mut self, message: impl Into<String>) -> Result<()> {
        let msg = message.into();
        match self {
            Reporter::Console => {
                println!("- {msg}");
                Ok(())
            }
            Reporter::Tui(reporter) => reporter.step(msg),
        }
    }

    pub fn pick_from_list(&mut self, title: &str, items: &[String]) -> Result<Option<usize>> {
        match self {
            Reporter::Console => crate::tui::pick_from_list(title, items),
            Reporter::Tui(reporter) => reporter.pick_from_list(title, items),
        }
    }

    pub fn tick(&mut self) -> Result<()> {
        match self {
            Reporter::Console => Ok(()),
            Reporter::Tui(reporter) => reporter.render(),
        }
    }

    pub fn finish(self, message: impl Into<String>) -> Result<()> {
        match self {
            Reporter::Console => {
                println!("- {}", message.into());
                Ok(())
            }
            Reporter::Tui(mut reporter) => {
                reporter.step(message.into())?;
                reporter.finish()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstallStep {
    Resolving,
    Downloading,
    Unpacking,
    Verifying,
    Linking,
    PostInstall,
}

impl InstallStep {
    const ALL: [InstallStep; 6] = [
        InstallStep::Resolving,
        InstallStep::Downloading,
        InstallStep::Unpacking,
        InstallStep::Verifying,
        InstallStep::Linking,
        InstallStep::PostInstall,
    ];

    fn label(self) -> &'static str {
        match self {
            InstallStep::Resolving => "Resolving source",
            InstallStep::Downloading => "Downloading",
            InstallStep::Unpacking => "Unpacking",
            InstallStep::Verifying => "Verifying",
            InstallStep::Linking => "Linking",
            InstallStep::PostInstall => "Post-install checks",
        }
    }

    fn index(self) -> usize {
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
enum SkillState {
    Queued,
    Running,
    Done,
    Failed,
}

#[derive(Clone, Debug)]
struct SkillEntry {
    name: String,
    version: Option<String>,
    requested: String,
    source: String,
    commit: String,
    state: SkillState,
    current_step: Option<InstallStep>,
    step_details: Vec<Option<String>>,
    error: Option<String>,
}

impl SkillEntry {
    fn from_queue(entry: QueuedSkill) -> Self {
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

    fn progress(&self) -> f64 {
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

pub struct TuiReporter {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    context: String,
    skills: Vec<SkillEntry>,
    view: Vec<usize>,
    list_state: ListState,
    active_skill: Option<usize>,
    last_list_height: u16,
    overlay: Option<Overlay>,
    filter: Option<String>,
    should_quit: bool,
    finished: bool,
}

#[derive(Clone, Debug)]
enum Overlay {
    QuitConfirm,
    Search { query: String },
}

#[derive(Clone, Debug)]
struct RenderSnapshot {
    context: String,
    skills: Vec<SkillEntry>,
    view: Vec<usize>,
    list_state: ListState,
    overlay: Option<Overlay>,
    active_skill: Option<usize>,
}

impl RenderSnapshot {
    fn from_reporter(reporter: &TuiReporter, view: Vec<usize>) -> Self {
        Self {
            context: reporter.context.clone(),
            skills: reporter.skills.clone(),
            view,
            list_state: reporter.list_state,
            overlay: reporter.overlay.clone(),
            active_skill: reporter.active_skill,
        }
    }
}

impl TuiReporter {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let stdout = io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let viewport_height = size()?.1.saturating_sub(1).max(1);
        let terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(viewport_height),
            },
        )?;

        Ok(Self {
            terminal,
            context: "skill install".to_string(),
            skills: Vec::new(),
            view: Vec::new(),
            list_state: ListState::default(),
            active_skill: None,
            last_list_height: 0,
            overlay: None,
            filter: None,
            should_quit: false,
            finished: false,
        })
    }

    fn set_context(&mut self, context: String) -> Result<()> {
        self.context = context;
        self.render()
    }

    fn queue_skills(&mut self, skills: Vec<QueuedSkill>) -> Result<()> {
        self.skills = skills.into_iter().map(SkillEntry::from_queue).collect();
        self.refresh_view();
        if !self.view.is_empty() {
            self.list_state.select(Some(0));
        }
        self.render()
    }

    fn begin_skill(&mut self, index: usize) -> Result<()> {
        if let Some(entry) = self.skills.get_mut(index) {
            entry.state = SkillState::Running;
            entry.current_step = Some(InstallStep::Resolving);
        }
        self.active_skill = Some(index);
        self.select_skill(index);
        self.render()
    }

    fn update_active_skill(&mut self, update: SkillUpdate) -> Result<()> {
        if let Some(index) = self.active_skill
            && let Some(entry) = self.skills.get_mut(index)
        {
            if let Some(name) = update.name {
                entry.name = name;
            }
            if let Some(version) = update.version {
                entry.version = Some(version);
            }
            self.refresh_view();
        }
        self.render()
    }

    fn finish_skill(&mut self) -> Result<()> {
        if let Some(index) = self.active_skill
            && let Some(entry) = self.skills.get_mut(index)
        {
            entry.state = SkillState::Done;
            entry.current_step = None;
        }
        self.render()
    }

    fn fail_active_skill(&mut self, error: String) -> Result<()> {
        if let Some(index) = self.active_skill
            && let Some(entry) = self.skills.get_mut(index)
        {
            entry.state = SkillState::Failed;
            entry.error = Some(error.clone());
        }
        self.render()
    }

    fn step(&mut self, message: String) -> Result<()> {
        if let Some(index) = self.active_skill {
            self.apply_skill_step(index, &message);
        }
        self.render()
    }

    fn render(&mut self) -> Result<()> {
        let size = self.terminal.size()?;
        let layout = RenderLayout::from_area(safe_area(size.into()));
        self.last_list_height = layout.list_height;
        self.clamp_list_offset();

        let view = self.view.clone();
        let snapshot = RenderSnapshot::from_reporter(self, view);
        self.terminal.draw(|frame| {
            snapshot.render_frame(frame, layout);
        })?;

        self.handle_events()?;
        if self.should_quit {
            return Err(anyhow!("install cancelled"));
        }
        Ok(())
    }

    fn pick_from_list(&mut self, title: &str, items: &[String]) -> Result<Option<usize>> {
        if items.is_empty() {
            return Ok(None);
        }

        let mut state = ListState::default();
        state.select(Some(0));

        loop {
            self.terminal.draw(|frame| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([Constraint::Length(3), Constraint::Min(1)].as_ref())
                    .split(frame.area());

                let help = Paragraph::new("Up/Down: move  Enter: select  Esc/q: cancel")
                    .block(Block::default().borders(Borders::ALL).title("Controls"));
                frame.render_widget(help, chunks[0]);

                let list_items: Vec<ListItem> = items
                    .iter()
                    .map(|item| ListItem::new(item.as_str()))
                    .collect();

                let list = List::new(list_items)
                    .block(Block::default().borders(Borders::ALL).title(title))
                    .highlight_style(Style::new().add_modifier(Modifier::REVERSED));

                frame.render_stateful_widget(list, chunks[1], &mut state);
            })?;

            if event::poll(Duration::from_millis(250))?
                && let Event::Key(key) = event::read()?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Up => {
                        let next = match state.selected() {
                            Some(0) | None => items.len() - 1,
                            Some(idx) => idx - 1,
                        };
                        state.select(Some(next));
                    }
                    KeyCode::Down => {
                        let next = match state.selected() {
                            Some(idx) if idx + 1 < items.len() => idx + 1,
                            _ => 0,
                        };
                        state.select(Some(next));
                    }
                    KeyCode::Enter => {
                        return Ok(state.selected());
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {
                        return Ok(None);
                    }
                    _ => {}
                }
            }
        }
    }

    fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        disable_raw_mode()?;
        self.terminal.show_cursor()?;
        Ok(())
    }

    fn handle_events(&mut self) -> Result<()> {
        while event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if self.handle_overlay_key(key.code)? {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => {
                        if self.installs_running() {
                            self.overlay = Some(Overlay::QuitConfirm);
                        } else {
                            self.should_quit = true;
                        }
                    }
                    KeyCode::Up => {
                        self.select_prev();
                    }
                    KeyCode::Down => {
                        self.select_next();
                    }
                    KeyCode::PageUp => {
                        let delta = self.last_list_height.saturating_sub(1).max(1) as usize;
                        self.select_page_up(delta);
                    }
                    KeyCode::PageDown => {
                        let delta = self.last_list_height.saturating_sub(1).max(1) as usize;
                        self.select_page_down(delta);
                    }
                    KeyCode::Char('k') => {
                        self.select_prev();
                    }
                    KeyCode::Char('j') => {
                        self.select_next();
                    }
                    KeyCode::Char('/') => {
                        self.overlay = Some(Overlay::Search {
                            query: String::new(),
                        });
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn handle_overlay_key(&mut self, code: KeyCode) -> Result<bool> {
        let Some(overlay) = self.overlay.clone() else {
            return Ok(false);
        };
        match overlay {
            Overlay::QuitConfirm => match code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    self.should_quit = true;
                    self.overlay = None;
                }
                KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.overlay = None;
                }
                _ => {}
            },
            Overlay::Search { mut query } => match code {
                KeyCode::Esc => {
                    self.overlay = None;
                }
                KeyCode::Enter => {
                    let trimmed = query.trim().to_string();
                    self.filter = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    };
                    self.refresh_view();
                    self.overlay = None;
                }
                KeyCode::Backspace => {
                    query.pop();
                    self.overlay = Some(Overlay::Search { query });
                }
                KeyCode::Char(ch) => {
                    if !ch.is_control() {
                        query.push(ch);
                        self.overlay = Some(Overlay::Search { query });
                    }
                }
                _ => {}
            },
        }
        Ok(true)
    }

    fn installs_running(&self) -> bool {
        self.skills
            .iter()
            .any(|skill| matches!(skill.state, SkillState::Queued | SkillState::Running))
    }
}

impl RenderSnapshot {
    fn render_frame(&self, frame: &mut ratatui::Frame, layout: RenderLayout) {
        if layout.header.height > 0 {
            self.render_header(frame, layout.header);
        }
        if let Some(gauge) = layout.gauge {
            self.render_gauge(frame, gauge);
        }
        if layout.list.height > 0 {
            self.render_list(frame, layout.list);
        }

        if let Some(overlay) = &self.overlay {
            self.render_overlay(frame, overlay);
        }
    }

    fn render_header(&self, frame: &mut ratatui::Frame, area: Rect) {
        let ratio = self.aggregate_progress();
        let percent = format!("{:.0}%", ratio * 100.0);
        let mut spans = vec![
            Span::from("Installing skills... "),
            Span::from(format!("({percent})")).cyan().bold(),
        ];
        if !self.context.is_empty() {
            spans.push("  ".into());
            spans.push(self.context.clone().dim().into());
        }
        let header = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
        frame.render_widget(header, area);
    }

    fn render_gauge(&self, frame: &mut ratatui::Frame, area: Rect) {
        let ratio = self.aggregate_progress();
        let label = format!("{:.0}%", ratio * 100.0);
        let gauge = LineGauge::default()
            .ratio(ratio)
            .label(label)
            .filled_style(Style::new().fg(Color::Cyan));
        frame.render_widget(gauge, area);
    }

    fn render_list(&self, frame: &mut ratatui::Frame, area: Rect) {
        let list_area = area;
        if self.view.is_empty() {
            let placeholder = Paragraph::new("Resolving install queue...")
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: false });
            frame.render_widget(placeholder, list_area);
            return;
        }

        let items = self.build_skill_items(list_area.width);
        let list = List::new(items);
        let mut state = self.list_state;
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    fn build_skill_items(&self, width: u16) -> Vec<ListItem<'_>> {
        let expanded = self.expanded_skill_index();
        let mut items = Vec::with_capacity(self.view.len());

        for &index in &self.view {
            let Some(skill) = self.skills.get(index) else {
                continue;
            };
            if Some(index) == expanded {
                items.push(ListItem::new(self.expanded_lines(skill, width)));
            } else {
                items.push(ListItem::new(vec![self.collapsed_line(skill)]));
            }
        }

        items
    }

    fn collapsed_line(&self, skill: &SkillEntry) -> Line<'_> {
        let status = self.skill_status_span(skill);
        let name = match skill.state {
            SkillState::Failed => Span::from(skill.name.clone()).red(),
            _ => Span::from(skill.name.clone()).dim(),
        };
        let mut spans = vec![status, " ".into(), name];
        if let Some(label) = self.version_label(skill) {
            spans.push(" ".into());
            spans.push(Span::from(label).dim());
        }
        Line::from(spans)
    }

    fn expanded_lines(&self, skill: &SkillEntry, width: u16) -> Vec<Line<'_>> {
        let separator = self.separator_line(width);
        let mut lines = Vec::new();

        let summary_status = self.skill_status_span(skill);
        let summary_name = Span::from(skill.name.clone()).cyan().bold();
        let summary_version = self
            .version_label(skill)
            .map(|label| Span::from(label).dim());
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
        let mut progress = vec![Span::from(skill.name.clone()).cyan().bold()];
        if let Some(label) = self.version_label(skill) {
            progress.push(" ".into());
            progress.push(Span::from(label).dim());
        }
        progress.push("  ".into());
        progress.push(Span::from(format!("({percent})")).cyan());
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
            let status = self.step_status(skill, *step);
            let label = match status {
                StepStatus::Active => Span::from(step.label()).cyan().bold(),
                StepStatus::Done => Span::from(step.label()).green().dim(),
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
                        Span::from(trim_detail(&detail)).cyan(),
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

    fn skill_status_span(&self, skill: &SkillEntry) -> Span<'static> {
        match skill.state {
            SkillState::Queued => "[ ]".dim().into(),
            SkillState::Running => "[>]".cyan().into(),
            SkillState::Done => "[✓]".green().into(),
            SkillState::Failed => "[✗]".red().into(),
        }
    }

    fn step_status(&self, skill: &SkillEntry, step: InstallStep) -> StepStatus {
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

    fn expanded_skill_index(&self) -> Option<usize> {
        let Some(index) = self.active_skill else {
            return None;
        };
        self.skills.get(index).and_then(|skill| {
            if matches!(skill.state, SkillState::Running | SkillState::Failed) {
                Some(index)
            } else {
                None
            }
        })
    }

    fn separator_line(&self, width: u16) -> Line<'_> {
        let count = width.max(10) as usize;
        Line::from("-".repeat(count))
    }

    fn version_label(&self, skill: &SkillEntry) -> Option<String> {
        if let Some(version) = &skill.version {
            return Some(format!("@{version}"));
        }
        if skill.requested.trim().is_empty() {
            return None;
        }
        Some(skill.requested.clone())
    }

    fn render_overlay(&self, frame: &mut ratatui::Frame, overlay: &Overlay) {
        let area = centered_rect(60, 20, frame.area());
        frame.render_widget(Clear, area);
        let (title, body) = match overlay {
            Overlay::QuitConfirm => (
                "Confirm Quit",
                "Install in progress. Quit? (y/n)".to_string(),
            ),
            Overlay::Search { query } => (
                "Search",
                if query.is_empty() {
                    "/".to_string()
                } else {
                    format!("/{query}")
                },
            ),
        };
        let block = Block::default().borders(Borders::ALL).title(title);
        frame.render_widget(block.clone(), area);
        let inner = block.inner(area);
        let text = Paragraph::new(body).alignment(Alignment::Center);
        frame.render_widget(text, inner);
    }

    fn aggregate_progress(&self) -> f64 {
        if self.skills.is_empty() {
            return 0.0;
        }
        let total: f64 = self.skills.iter().map(|skill| skill.progress()).sum();
        total / self.skills.len() as f64
    }
}

impl TuiReporter {
    fn refresh_view(&mut self) {
        let filter = self.filter.as_ref().map(|value| value.to_lowercase());
        self.view = self
            .skills
            .iter()
            .enumerate()
            .filter(|(_, skill)| {
                filter
                    .as_ref()
                    .map(|filter| skill.name.to_lowercase().contains(filter))
                    .unwrap_or(true)
            })
            .map(|(index, _)| index)
            .collect();
        if self.view.is_empty() {
            self.list_state.select(None);
            return;
        }
        let selected = self.list_state.selected().unwrap_or(0);
        if selected >= self.view.len() {
            self.list_state.select(Some(0));
        }
    }

    fn select_prev(&mut self) {
        if self.view.is_empty() {
            return;
        }
        let next = match self.list_state.selected() {
            Some(0) | None => self.view.len() - 1,
            Some(idx) => idx - 1,
        };
        self.list_state.select(Some(next));
    }

    fn select_next(&mut self) {
        if self.view.is_empty() {
            return;
        }
        let next = match self.list_state.selected() {
            Some(idx) if idx + 1 < self.view.len() => idx + 1,
            _ => 0,
        };
        self.list_state.select(Some(next));
    }

    fn select_page_up(&mut self, delta: usize) {
        if self.view.is_empty() {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        let next = current.saturating_sub(delta);
        self.list_state.select(Some(next));
    }

    fn select_page_down(&mut self, delta: usize) {
        if self.view.is_empty() {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        let max_index = self.view.len().saturating_sub(1);
        let next = (current + delta).min(max_index);
        self.list_state.select(Some(next));
    }

    fn select_skill(&mut self, index: usize) {
        if let Some(position) = self.view.iter().position(|&value| value == index) {
            self.list_state.select(Some(position));
        }
    }

    fn clamp_list_offset(&mut self) {
        let Some(selected) = self.list_state.selected() else {
            return;
        };
        let height = self.last_list_height.max(1) as usize;
        let offset = self.list_state.offset();
        if selected < offset {
            *self.list_state.offset_mut() = selected;
        } else if selected >= offset + height {
            *self.list_state.offset_mut() = selected + 1 - height;
        }
    }

    fn apply_skill_step(&mut self, index: usize, message: &str) {
        let Some(skill) = self.skills.get_mut(index) else {
            return;
        };

        if let Some(step) = map_step(message) {
            skill.current_step = Some(step);
            skill.state = SkillState::Running;
            let detail = trim_detail(message);
            if let Some(slot) = skill.step_details.get_mut(step.index()) {
                *slot = Some(detail);
            }
        }
    }
}

impl Drop for TuiReporter {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StepStatus {
    Pending,
    Active,
    Done,
}

fn map_step(message: &str) -> Option<InstallStep> {
    if message.starts_with("Parsing reference")
        || message.starts_with("Resolving git URL")
        || message.starts_with("Falling back to GitHub shorthand")
        || message.starts_with("Checking registries")
        || message.starts_with("Resolving version")
        || message.starts_with("Resolving commit")
        || message.starts_with("Preparing local mirror cache")
        || message.starts_with("Scanning repo")
        || message.starts_with("Ensuring mirror cache")
    {
        return Some(InstallStep::Resolving);
    }
    if message.starts_with("Fetching commit") {
        return Some(InstallStep::Downloading);
    }
    if message.starts_with("Extracting skill directory") {
        return Some(InstallStep::Unpacking);
    }
    if message.starts_with("Validating SKILL.md") {
        return Some(InstallStep::Verifying);
    }
    if message.starts_with("Copying files") {
        return Some(InstallStep::Linking);
    }
    if message.starts_with("Updating lockfile") {
        return Some(InstallStep::PostInstall);
    }
    None
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

fn safe_area(area: Rect) -> Rect {
    Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(1),
        area.height.saturating_sub(1),
    )
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
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
