use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Modifier, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::agents::{AgentTarget, Scope, TargetKey};
use super::git_tracking::TrackingPreference;
use super::{ApplySelection, ApplySkill, SkillKey, TrackingContext};
use skill_core::ui::components::{Footer, Header};
use skill_core::ui::interaction;
use skill_core::ui::terminal::{UiTerminal, safe_area, setup_inline_terminal, teardown_terminal};
use skill_core::ui::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Targets,
    Skills,
}

impl Step {
    fn index(self) -> usize {
        match self {
            Step::Targets => 1,
            Step::Skills => 2,
        }
    }
}

const TOTAL_STEPS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApplyUiMode {
    Apply,
    Unapply,
}

impl ApplyUiMode {
    fn from_unapply(unapply: bool) -> Self {
        if unapply { Self::Unapply } else { Self::Apply }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Apply => "Apply",
            Self::Unapply => "Unapply",
        }
    }

    fn verb(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Unapply => "unapply",
        }
    }

    fn selection_hint(self) -> &'static str {
        match self {
            Self::Apply => {
                "Selected skills will be applied; unselected applied skills will be unapplied."
            }
            Self::Unapply => "Selected skills will be unapplied.",
        }
    }
}

pub(super) fn run_apply_ui(
    targets: &[AgentTarget],
    skills: &[ApplySkill],
    applied: &HashMap<(SkillKey, TargetKey), bool>,
    tracking: &TrackingContext,
    unapply: bool,
) -> Result<Option<ApplySelection>> {
    let mode = ApplyUiMode::from_unapply(unapply);
    let mut terminal = setup_inline_terminal()?;
    let result = run_apply_loop(&mut terminal, targets, skills, applied, tracking, mode);
    teardown_terminal(&mut terminal)?;
    result
}

fn run_apply_loop(
    terminal: &mut UiTerminal,
    targets: &[AgentTarget],
    skills: &[ApplySkill],
    applied: &HashMap<(SkillKey, TargetKey), bool>,
    tracking: &TrackingContext,
    mode: ApplyUiMode,
) -> Result<Option<ApplySelection>> {
    let mut step = Step::Targets;
    // Single target selection
    let mut selected_target: Option<TargetKey> = targets
        .iter()
        .find(|target| target.default_selected)
        .map(|target| target.key.clone());

    let mut target_state = ListState::default();
    let initial_index = targets
        .iter()
        .position(|t| Some(&t.key) == selected_target.as_ref())
        .unwrap_or(0);
    target_state.select(Some(initial_index));

    let mut skill_state = ListState::default();
    skill_state.select(Some(0));

    let mut selected_skills = HashSet::new();
    let mut tracking_preferences = HashMap::new();

    loop {
        terminal.draw(|frame| {
            let area = safe_area(frame.area());
            match step {
                Step::Targets => {
                    render_targets(frame, area, targets, &selected_target, &target_state, mode);
                }
                Step::Skills => {
                    let context = SkillsRenderContext {
                        target: selected_target
                            .as_ref()
                            .expect("target must be selected in skills step"),
                        skills,
                        applied,
                        selected_skills: &selected_skills,
                        tracking,
                        tracking_preferences: &tracking_preferences,
                        state: &skill_state,
                        mode,
                    };
                    render_skills(frame, area, &context);
                }
            }
        })?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            match step {
                Step::Targets => {
                    if handle_list_keys(&mut target_state, targets.len(), key.code) {
                        if let Some(idx) = target_state.selected()
                            && let Some(target) = targets.get(idx)
                        {
                            if target.enabled {
                                selected_target = Some(target.key.clone());
                            } else {
                                selected_target = None;
                            }
                        }
                        continue;
                    }
                    match key.code {
                        KeyCode::Enter => {
                            if selected_target.is_none() {
                                continue;
                            }
                            // Initialize skills based on the single selected target
                            if selected_skills.is_empty()
                                && let Some(target_key) = &selected_target
                            {
                                selected_skills =
                                    default_skill_selection(skills, target_key, applied);
                                tracking_preferences =
                                    default_tracking_preferences(skills, target_key, tracking);
                            }
                            step = Step::Skills;
                        }
                        KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                        _ => {}
                    }
                }
                Step::Skills => {
                    if handle_list_keys(&mut skill_state, skills.len(), key.code) {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char(' ') => {
                            toggle_skill(skills, &mut selected_skills, &skill_state);
                        }
                        KeyCode::Char('a') | KeyCode::Char('A') => {
                            selected_skills = skills
                                .iter()
                                .filter(|skill| skill.source_exists)
                                .map(|skill| skill.key.clone())
                                .collect();
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            selected_skills.clear();
                        }
                        KeyCode::Char('g') | KeyCode::Char('G') => {
                            toggle_tracking(
                                skills,
                                tracking,
                                &selected_target,
                                &mut tracking_preferences,
                                &skill_state,
                            );
                        }
                        KeyCode::Enter => {
                            let selected_tracking = selected_target
                                .as_ref()
                                .and_then(|target_key| tracking.for_target(target_key))
                                .and_then(|state| state.repo_root.as_ref())
                                .map(|_| tracking_preferences.clone())
                                .unwrap_or_default();
                            return Ok(Some(ApplySelection {
                                targets: selected_target.iter().cloned().collect(),
                                skills: selected_skills.iter().cloned().collect(),
                                tracking: selected_tracking,
                            }));
                        }
                        KeyCode::Backspace | KeyCode::Char('b') => {
                            selected_skills.clear();
                            tracking_preferences.clear();
                            step = Step::Targets;
                        }
                        KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                        _ => {}
                    }
                }
            }
        }
    }
}

fn handle_list_keys(state: &mut ListState, len: usize, key: KeyCode) -> bool {
    match key {
        KeyCode::Up => {
            let next = match state.selected() {
                Some(0) | None => len.saturating_sub(1),
                Some(idx) => idx.saturating_sub(1),
            };
            state.select(Some(next));
            true
        }
        KeyCode::Down => {
            let next = match state.selected() {
                Some(idx) if idx + 1 < len => idx + 1,
                _ => 0,
            };
            state.select(Some(next));
            true
        }
        _ => false,
    }
}

fn toggle_skill(skills: &[ApplySkill], selected: &mut HashSet<SkillKey>, state: &ListState) {
    let Some(idx) = state.selected() else {
        return;
    };
    let Some(skill) = skills.get(idx) else {
        return;
    };
    if !skill.source_exists {
        return;
    }
    if selected.contains(&skill.key) {
        selected.remove(&skill.key);
    } else {
        selected.insert(skill.key.clone());
    }
}

fn default_skill_selection(
    skills: &[ApplySkill],
    target_key: &TargetKey,
    applied: &HashMap<(SkillKey, TargetKey), bool>,
) -> HashSet<SkillKey> {
    let mut selected = HashSet::new();
    for skill in skills {
        let already = applied
            .get(&(skill.key.clone(), target_key.clone()))
            .copied()
            .unwrap_or(false);
        if already {
            selected.insert(skill.key.clone());
        }
    }
    selected
}

fn default_tracking_preferences(
    skills: &[ApplySkill],
    target_key: &TargetKey,
    tracking: &TrackingContext,
) -> HashMap<SkillKey, TrackingPreference> {
    let Some(state) = tracking.for_target(target_key) else {
        return HashMap::new();
    };
    let Some(_repo_root) = state.repo_root.as_ref() else {
        return HashMap::new();
    };

    skills
        .iter()
        .map(|skill| {
            let preference = state
                .initial
                .get(&skill.key)
                .copied()
                .unwrap_or(TrackingPreference::NotTracked);
            (skill.key.clone(), preference)
        })
        .collect()
}

fn toggle_tracking(
    skills: &[ApplySkill],
    tracking: &TrackingContext,
    selected_target: &Option<TargetKey>,
    preferences: &mut HashMap<SkillKey, TrackingPreference>,
    state: &ListState,
) {
    let Some(target_key) = selected_target.as_ref() else {
        return;
    };
    let Some(target_state) = tracking.for_target(target_key) else {
        return;
    };
    if target_state.repo_root.is_none() {
        return;
    }

    let Some(index) = state.selected() else {
        return;
    };
    let Some(skill) = skills.get(index) else {
        return;
    };

    let entry = preferences.entry(skill.key.clone()).or_insert_with(|| {
        target_state
            .initial
            .get(&skill.key)
            .copied()
            .unwrap_or(TrackingPreference::NotTracked)
    });
    *entry = match entry {
        TrackingPreference::Tracked => TrackingPreference::NotTracked,
        TrackingPreference::NotTracked => TrackingPreference::Tracked,
    };
}

fn render_targets(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    targets: &[AgentTarget],
    selected_target: &Option<TargetKey>,
    state: &ListState,
    mode: ApplyUiMode,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1), // Header
                Constraint::Length(3), // Description
                Constraint::Min(1),    // List
                Constraint::Length(1), // Spacer
                Constraint::Length(1), // Footer
            ]
            .as_ref(),
        )
        .split(area);

    let header_title = format!(
        "{title} skills ({step}/{TOTAL_STEPS})",
        title = mode.title(),
        step = Step::Targets.index()
    );
    let header = Header::new(&header_title);
    frame.render_widget(header, chunks[0]);

    let description = Paragraph::new(vec![
        Line::from(
            Span::from(format!(
                "Step {step}/{TOTAL_STEPS}: Choose where to {verb} skills.",
                step = Step::Targets.index(),
                verb = mode.verb()
            ))
            .dim(),
        ),
        Line::from(Span::from("Detected agents are marked; unsupported options are dimmed.").dim()),
    ])
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: false });
    frame.render_widget(description, chunks[1]);

    let footer = Footer::new(interaction::apply_targets_footer());
    frame.render_widget(footer, chunks[4]);

    let list_items = targets
        .iter()
        .map(|target| {
            // Radio button style: ( ) or (*)
            let is_selected = selected_target.as_ref() == Some(&target.key);
            let marker = if is_selected { "(*)" } else { "( )" };

            let mut spans = vec![
                Span::from(marker),
                " ".into(),
                Span::from(target.label.clone()),
                " ".into(),
            ];

            if target.key.scope == Scope::Global || target.detected {
                spans.push(Span::from(format!("({path})", path = target.base_dir.display())).dim());
            } else {
                spans.push(Span::from("(skill folder not found)").dim());
            }

            if target.detected {
                spans.push(" ".into());
                spans.push(Span::from("detected").style(theme::accent_style()).bold());
            }
            if !target.enabled {
                spans.push(" ".into());
                spans.push(Span::from("unsupported").dim());
            }
            let line = Line::from(spans);
            if target.enabled {
                ListItem::new(line)
            } else {
                ListItem::new(line.dim())
            }
        })
        .collect::<Vec<_>>();

    let list = List::new(list_items).highlight_style(
        theme::accent_style()
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::REVERSED),
    );
    let mut state = *state;
    frame.render_stateful_widget(list, chunks[2], &mut state);
}

struct SkillsRenderContext<'a> {
    target: &'a TargetKey,
    skills: &'a [ApplySkill],
    applied: &'a HashMap<(SkillKey, TargetKey), bool>,
    selected_skills: &'a HashSet<SkillKey>,
    tracking: &'a TrackingContext,
    tracking_preferences: &'a HashMap<SkillKey, TrackingPreference>,
    state: &'a ListState,
    mode: ApplyUiMode,
}

fn render_skills(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    context: &SkillsRenderContext<'_>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1), // Header
                Constraint::Length(2), // Stats
                Constraint::Length(1), // Spacer (Header/List gap)
                Constraint::Min(1),    // List
                Constraint::Length(1), // Spacer (List/Footer gap)
                Constraint::Length(1), // Footer
            ]
            .as_ref(),
        )
        .split(area);

    let header_title = format!(
        "{title} skills ({step}/{TOTAL_STEPS})",
        title = context.mode.title(),
        step = Step::Skills.index()
    );
    let header = Header::new(&header_title);
    frame.render_widget(header, chunks[0]);

    let stats = Paragraph::new(vec![
        Line::from(vec![
            Span::from("Target: ").dim(),
            Span::from(context.target.label()).style(theme::accent_style()),
        ]),
        Line::from(vec![
            Span::from(context.mode.selection_hint()).dim(),
            " ".into(),
            git_tracking_hint(context).dim(),
        ]),
    ])
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: false });
    frame.render_widget(stats, chunks[1]);

    let footer = Footer::new(interaction::apply_skills_footer(
        context.mode.verb(),
        tracking_is_available(context),
    ));
    frame.render_widget(footer, chunks[5]);

    let list_items = context
        .skills
        .iter()
        .map(|skill| {
            let is_selected = context.selected_skills.contains(&skill.key);
            let checked = if is_selected { "[x]" } else { "[ ]" };
            let mut spans = vec![
                Span::from(checked),
                " ".into(),
                Span::from(skill.key.label()),
            ];
            if !skill.source_exists {
                spans.push(" ".into());
                spans.push(Span::from("missing source").dim());
            }

            let is_installed = context
                .applied
                .get(&(skill.key.clone(), context.target.clone()))
                .copied()
                .unwrap_or(false);

            if let Some(status) = skill_status_span(context.mode, is_selected, is_installed) {
                spans.push(" ".into());
                spans.push(status);
            }

            if let Some(tracking_status) = tracking_status_span(context, skill) {
                spans.push(" ".into());
                spans.push(tracking_status);
            }

            let line = Line::from(spans);
            if skill.source_exists {
                ListItem::new(line)
            } else {
                ListItem::new(line.dim())
            }
        })
        .collect::<Vec<_>>();

    let list = List::new(list_items).highlight_style(
        theme::accent_style()
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::REVERSED),
    );

    let mut state = *context.state;
    frame.render_stateful_widget(list, chunks[3], &mut state);
}

fn tracking_is_available(context: &SkillsRenderContext<'_>) -> bool {
    context
        .tracking
        .for_target(context.target)
        .and_then(|target| target.repo_root.as_ref())
        .is_some()
}

fn git_tracking_hint(context: &SkillsRenderContext<'_>) -> &'static str {
    if tracking_is_available(context) {
        "Press g to toggle Git tracking (default: not tracked)."
    } else {
        "Git tracking unavailable for this target."
    }
}

fn tracking_status_span(
    context: &SkillsRenderContext<'_>,
    skill: &ApplySkill,
) -> Option<Span<'static>> {
    if !tracking_is_available(context) {
        return Some(Span::from("(tracking unavailable)").dim());
    }
    let preference = context
        .tracking_preferences
        .get(&skill.key)
        .copied()
        .or_else(|| {
            context
                .tracking
                .for_target(context.target)
                .and_then(|target| target.initial.get(&skill.key).copied())
        })
        .unwrap_or(TrackingPreference::NotTracked);

    match preference {
        TrackingPreference::Tracked => Some(Span::from("(tracked)").dim()),
        TrackingPreference::NotTracked => Some(
            Span::from("(not tracked)")
                .style(theme::accent_style())
                .bold(),
        ),
    }
}

fn skill_status_span(
    mode: ApplyUiMode,
    is_selected: bool,
    is_applied: bool,
) -> Option<Span<'static>> {
    match mode {
        ApplyUiMode::Apply => {
            if is_selected {
                if is_applied {
                    Some(Span::from("(Applied)").dim())
                } else {
                    Some(
                        Span::from("(Will Apply)")
                            .style(theme::success_style())
                            .bold(),
                    )
                }
            } else if is_applied {
                Some(
                    Span::from("(Will Unapply)")
                        .style(theme::error_style())
                        .bold(),
                )
            } else {
                None
            }
        }
        ApplyUiMode::Unapply => {
            if is_selected {
                if is_applied {
                    Some(
                        Span::from("(Will Unapply)")
                            .style(theme::error_style())
                            .bold(),
                    )
                } else {
                    Some(Span::from("(Not Applied)").dim())
                }
            } else if is_applied {
                Some(Span::from("(Applied)").dim())
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::agents::AgentId;
    use super::*;
    use std::path::PathBuf;

    fn build_skill(name: &str) -> ApplySkill {
        ApplySkill {
            key: SkillKey {
                source_id: "acme".to_string(),
                name: name.to_string(),
            },
            source_dir: PathBuf::from("/tmp"),
            source_exists: true,
        }
    }

    fn build_target() -> TargetKey {
        TargetKey {
            agent: AgentId::Codex,
            scope: Scope::Project,
        }
    }

    #[test]
    fn when_toggling_tracking_should_flip_state_for_highlighted_skill() {
        let skill = build_skill("echo-skill");
        let skills = vec![skill.clone()];
        let target = build_target();
        let mut initial = HashMap::new();
        initial.insert(skill.key.clone(), TrackingPreference::Tracked);

        let mut by_target = HashMap::new();
        by_target.insert(
            target.clone(),
            super::super::TargetTracking {
                repo_root: Some(PathBuf::from("/tmp/repo")),
                initial,
            },
        );
        let tracking = TrackingContext { by_target };
        let selected_target = Some(target);
        let mut preferences = HashMap::new();
        let mut state = ListState::default();
        state.select(Some(0));

        toggle_tracking(
            &skills,
            &tracking,
            &selected_target,
            &mut preferences,
            &state,
        );
        assert_eq!(
            preferences.get(&skill.key).copied(),
            Some(TrackingPreference::NotTracked)
        );

        toggle_tracking(
            &skills,
            &tracking,
            &selected_target,
            &mut preferences,
            &state,
        );
        assert_eq!(
            preferences.get(&skill.key).copied(),
            Some(TrackingPreference::Tracked)
        );
    }

    #[test]
    fn when_tracking_unavailable_should_render_unavailable_badge() {
        let skill = build_skill("echo-skill");
        let target = build_target();
        let mut by_target = HashMap::new();
        by_target.insert(
            target.clone(),
            super::super::TargetTracking {
                repo_root: None,
                initial: HashMap::new(),
            },
        );
        let tracking = TrackingContext { by_target };
        let selected_skills = HashSet::new();
        let applied = HashMap::new();
        let prefs = HashMap::new();
        let state = ListState::default();
        let context = SkillsRenderContext {
            target: &target,
            skills: std::slice::from_ref(&skill),
            applied: &applied,
            selected_skills: &selected_skills,
            tracking: &tracking,
            tracking_preferences: &prefs,
            state: &state,
            mode: ApplyUiMode::Apply,
        };

        let status = tracking_status_span(&context, &skill).expect("tracking status");
        assert_eq!(status.content, "(tracking unavailable)");
    }
}
