use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

use crate::output::Output;
use crate::ui::install::{
    self, InstallSkill, InstallStep, InstallView, InstallViewModel, SkillState,
};
use crate::ui::picker;
use crate::ui::terminal::{UiTerminal, safe_area, setup_inline_terminal, teardown_terminal};

pub use crate::ui::install::QueuedSkill;

pub struct SkillUpdate {
    pub name: Option<String>,
    pub version: Option<String>,
}

pub struct Reporter {
    inner: TuiReporter,
}

impl Reporter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: TuiReporter::new()?,
        })
    }

    pub fn set_context(&mut self, context: impl Into<String>) -> Result<()> {
        self.inner.set_context(context.into())
    }

    pub fn queue_skills(&mut self, skills: Vec<QueuedSkill>) -> Result<()> {
        self.inner.queue_skills(skills)
    }

    pub fn begin_skill(&mut self, index: usize) -> Result<()> {
        self.inner.begin_skill(index)
    }

    pub fn update_active_skill(&mut self, update: SkillUpdate) -> Result<()> {
        self.inner.update_active_skill(update)
    }

    pub fn finish_skill(&mut self) -> Result<()> {
        self.inner.finish_skill()
    }

    pub fn fail_active_skill(&mut self, error: impl Into<String>) -> Result<()> {
        self.inner.fail_active_skill(error.into())
    }

    pub fn step(&mut self, message: impl Into<String>) -> Result<()> {
        self.inner.step(message.into())
    }

    pub fn pick_from_list(&mut self, title: &str, items: &[String]) -> Result<Option<usize>> {
        self.inner.pick_from_list(title, items)
    }

    pub fn tick(&mut self) -> Result<()> {
        self.inner.render()
    }

    pub fn finish(self, message: impl Into<String>) -> Result<()> {
        let mut reporter = self.inner;
        reporter.step(message.into())?;
        reporter.finish()
    }
}

impl Output for Reporter {
    fn line(&mut self, message: impl Into<String>) -> Result<()> {
        self.step(message)
    }
}

struct TuiReporter {
    terminal: UiTerminal,
    model: InstallViewModel,
    last_list_height: u16,
    should_quit: bool,
    finished: bool,
}

impl TuiReporter {
    fn new() -> Result<Self> {
        Ok(Self {
            terminal: setup_inline_terminal()?,
            model: InstallViewModel {
                context: "skill install".to_string(),
                ..InstallViewModel::default()
            },
            last_list_height: 0,
            should_quit: false,
            finished: false,
        })
    }

    fn set_context(&mut self, context: String) -> Result<()> {
        self.model.context = context;
        self.render()
    }

    fn queue_skills(&mut self, skills: Vec<QueuedSkill>) -> Result<()> {
        self.model.skills = skills.into_iter().map(InstallSkill::from_queue).collect();
        self.refresh_view();
        if !self.model.view.is_empty() {
            self.model.list_state.select(Some(0));
        }
        self.render()
    }

    fn begin_skill(&mut self, index: usize) -> Result<()> {
        if let Some(entry) = self.model.skills.get_mut(index) {
            entry.state = SkillState::Running;
            entry.current_step = Some(InstallStep::Resolving);
        }
        self.model.active_skill = Some(index);
        self.select_skill(index);
        self.render()
    }

    fn update_active_skill(&mut self, update: SkillUpdate) -> Result<()> {
        if let Some(index) = self.model.active_skill
            && let Some(entry) = self.model.skills.get_mut(index)
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
        if let Some(index) = self.model.active_skill
            && let Some(entry) = self.model.skills.get_mut(index)
        {
            entry.state = SkillState::Done;
            entry.current_step = None;
        }
        self.render()
    }

    fn fail_active_skill(&mut self, error: String) -> Result<()> {
        if let Some(index) = self.model.active_skill
            && let Some(entry) = self.model.skills.get_mut(index)
        {
            entry.state = SkillState::Failed;
            entry.error = Some(error);
        }
        self.render()
    }

    fn step(&mut self, message: String) -> Result<()> {
        if let Some(index) = self.model.active_skill {
            self.apply_skill_step(index, &message);
        }
        self.render()
    }

    fn render(&mut self) -> Result<()> {
        let area = safe_area(self.terminal.size()?.into());
        self.last_list_height = install::list_height(area);
        self.clamp_list_offset();

        let model = self.model.clone();
        self.terminal.draw(|frame| {
            InstallView::render(frame, area, &model);
        })?;

        self.handle_events()?;
        if self.should_quit {
            return Err(anyhow!("install cancelled"));
        }
        Ok(())
    }

    fn pick_from_list(&mut self, title: &str, items: &[String]) -> Result<Option<usize>> {
        let choice = picker::pick_from_list(&mut self.terminal, title, items)?;
        self.render()?;
        Ok(choice)
    }

    fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        teardown_terminal(&mut self.terminal)
    }

    fn handle_events(&mut self) -> Result<()> {
        while event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => {
                        self.should_quit = true;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.select_prev();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
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
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn refresh_view(&mut self) {
        self.model.view = (0..self.model.skills.len()).collect();
        if self.model.view.is_empty() {
            self.model.list_state.select(None);
            return;
        }
        let selected = self.model.list_state.selected().unwrap_or(0);
        if selected >= self.model.view.len() {
            self.model.list_state.select(Some(0));
        }
    }

    fn select_prev(&mut self) {
        if self.model.view.is_empty() {
            return;
        }
        let next = match self.model.list_state.selected() {
            Some(0) | None => self.model.view.len() - 1,
            Some(idx) => idx - 1,
        };
        self.model.list_state.select(Some(next));
    }

    fn select_next(&mut self) {
        if self.model.view.is_empty() {
            return;
        }
        let next = match self.model.list_state.selected() {
            Some(idx) if idx + 1 < self.model.view.len() => idx + 1,
            _ => 0,
        };
        self.model.list_state.select(Some(next));
    }

    fn select_page_up(&mut self, delta: usize) {
        if self.model.view.is_empty() {
            return;
        }
        let current = self.model.list_state.selected().unwrap_or(0);
        let next = current.saturating_sub(delta);
        self.model.list_state.select(Some(next));
    }

    fn select_page_down(&mut self, delta: usize) {
        if self.model.view.is_empty() {
            return;
        }
        let current = self.model.list_state.selected().unwrap_or(0);
        let max_index = self.model.view.len().saturating_sub(1);
        let next = (current + delta).min(max_index);
        self.model.list_state.select(Some(next));
    }

    fn select_skill(&mut self, index: usize) {
        if let Some(position) = self.model.view.iter().position(|&value| value == index) {
            self.model.list_state.select(Some(position));
        }
    }

    fn clamp_list_offset(&mut self) {
        let Some(selected) = self.model.list_state.selected() else {
            return;
        };
        let height = self.last_list_height.max(1) as usize;
        let offset = self.model.list_state.offset();
        if selected < offset {
            *self.model.list_state.offset_mut() = selected;
        } else if selected >= offset + height {
            *self.model.list_state.offset_mut() = selected + 1 - height;
        }
    }

    fn apply_skill_step(&mut self, index: usize, message: &str) {
        let Some(skill) = self.model.skills.get_mut(index) else {
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
