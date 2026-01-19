use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Modifier, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::agents::{AgentTarget, TargetKey};
use super::{ApplySelection, ApplySkill, SkillKey};
use skill_core::ui::terminal::{UiTerminal, safe_area, setup_inline_terminal, teardown_terminal};
use skill_core::ui::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Targets,
    Skills,
    Summary,
}

pub(super) fn run_apply_ui(
    targets: &[AgentTarget],
    skills: &[ApplySkill],
    applied: &HashMap<(SkillKey, TargetKey), bool>,
) -> Result<Option<ApplySelection>> {
    let mut terminal = setup_inline_terminal()?;
    let result = run_apply_loop(&mut terminal, targets, skills, applied);
    teardown_terminal(&mut terminal)?;
    result
}

fn run_apply_loop(
    terminal: &mut UiTerminal,
    targets: &[AgentTarget],
    skills: &[ApplySkill],
    applied: &HashMap<(SkillKey, TargetKey), bool>,
) -> Result<Option<ApplySelection>> {
    let mut step = Step::Targets;
    let mut target_state = ListState::default();
    target_state.select(Some(0));
    let mut skill_state = ListState::default();
    skill_state.select(Some(0));

    let mut selected_targets = targets
        .iter()
        .filter(|target| target.default_selected)
        .map(|target| target.key.clone())
        .collect::<HashSet<_>>();
    let mut selected_skills = HashSet::new();

    loop {
        terminal.draw(|frame| {
            let area = safe_area(frame.area());
            match step {
                Step::Targets => {
                    render_targets(frame, area, targets, &selected_targets, &target_state);
                }
                Step::Skills => {
                    let context = SkillsRenderContext {
                        targets,
                        skills,
                        applied,
                        selected_targets: &selected_targets,
                        selected_skills: &selected_skills,
                        state: &skill_state,
                    };
                    render_skills(frame, area, &context);
                }
                Step::Summary => {
                    render_summary(
                        frame,
                        area,
                        targets,
                        skills,
                        &selected_targets,
                        &selected_skills,
                    );
                }
            }
        })?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            match step {
                Step::Targets => {
                    if handle_list_keys(&mut target_state, targets.len(), key.code) {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char(' ') => {
                            toggle_target(targets, &mut selected_targets, &target_state);
                        }
                        KeyCode::Char('a') => {
                            selected_targets = targets
                                .iter()
                                .filter(|target| target.enabled)
                                .map(|target| target.key.clone())
                                .collect();
                        }
                        KeyCode::Char('n') => {
                            selected_targets.clear();
                        }
                        KeyCode::Enter => {
                            if selected_targets.is_empty() {
                                continue;
                            }
                            if selected_skills.is_empty() {
                                selected_skills =
                                    default_skill_selection(skills, &selected_targets, applied);
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
                        KeyCode::Char('a') => {
                            selected_skills = skills
                                .iter()
                                .filter(|skill| skill.source_exists)
                                .map(|skill| skill.key.clone())
                                .collect();
                        }
                        KeyCode::Char('n') => {
                            selected_skills.clear();
                        }
                        KeyCode::Enter => {
                            step = Step::Summary;
                        }
                        KeyCode::Backspace | KeyCode::Char('b') => {
                            step = Step::Targets;
                        }
                        KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                        _ => {}
                    }
                }
                Step::Summary => match key.code {
                    KeyCode::Enter => {
                        return Ok(Some(ApplySelection {
                            targets: selected_targets.iter().cloned().collect(),
                            skills: selected_skills.iter().cloned().collect(),
                        }));
                    }
                    KeyCode::Backspace | KeyCode::Char('b') => step = Step::Skills,
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                    _ => {}
                },
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

fn toggle_target(targets: &[AgentTarget], selected: &mut HashSet<TargetKey>, state: &ListState) {
    let Some(idx) = state.selected() else {
        return;
    };
    let Some(target) = targets.get(idx) else {
        return;
    };
    if !target.enabled {
        return;
    }
    if selected.contains(&target.key) {
        selected.remove(&target.key);
    } else {
        selected.insert(target.key.clone());
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
    targets: &HashSet<TargetKey>,
    applied: &HashMap<(SkillKey, TargetKey), bool>,
) -> HashSet<SkillKey> {
    let mut selected = HashSet::new();
    for skill in skills {
        let already = targets.iter().any(|target| {
            let target_key: TargetKey = target.clone();
            applied
                .get(&(skill.key.clone(), target_key))
                .copied()
                .unwrap_or(false)
        });
        if already {
            selected.insert(skill.key.clone());
        }
    }
    selected
}

fn render_targets(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    targets: &[AgentTarget],
    selected_targets: &HashSet<TargetKey>,
    state: &ListState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .split(area);

    let header = Paragraph::new(Line::from(vec![
        Span::from("Apply skills")
            .style(theme::accent_style())
            .bold(),
        "  ".into(),
        Span::from("Select agent scopes").dim(),
    ]))
    .alignment(Alignment::Left);
    frame.render_widget(header, chunks[0]);

    let help = Paragraph::new(vec![
        Line::from(
            Span::from("Up/Down: move  Space: toggle  a: all  n: none  Enter: next  Esc/q: cancel")
                .dim(),
        ),
        Line::from(Span::from("Detected agents are marked; unsupported options are dimmed.").dim()),
    ])
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: false });
    frame.render_widget(help, chunks[1]);

    let list_items = targets
        .iter()
        .map(|target| {
            let checked = if selected_targets.contains(&target.key) {
                "[x]"
            } else {
                "[ ]"
            };
            let mut spans = vec![
                Span::from(checked),
                " ".into(),
                Span::from(target.label.clone()),
            ];
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
    targets: &'a [AgentTarget],
    skills: &'a [ApplySkill],
    applied: &'a HashMap<(SkillKey, TargetKey), bool>,
    selected_targets: &'a HashSet<TargetKey>,
    selected_skills: &'a HashSet<SkillKey>,
    state: &'a ListState,
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
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .split(area);

    let target_count = context.selected_targets.len();
    let header = Paragraph::new(Line::from(vec![
        Span::from("Select skills")
            .style(theme::accent_style())
            .bold(),
        "  ".into(),
        Span::from(format!("Targets: {target_count}")).dim(),
    ]))
    .alignment(Alignment::Left);
    frame.render_widget(header, chunks[0]);

    let help = Paragraph::new(vec![
        Line::from(
            Span::from("Up/Down: move  Space: toggle  a: all  n: none  b: back  Enter: next").dim(),
        ),
        Line::from(
            Span::from("Skills already applied show a check for each selected target.").dim(),
        ),
    ])
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: false });
    frame.render_widget(help, chunks[1]);

    let list_items = context
        .skills
        .iter()
        .map(|skill| {
            let checked = if context.selected_skills.contains(&skill.key) {
                "[x]"
            } else {
                "[ ]"
            };
            let mut spans = vec![
                Span::from(checked),
                " ".into(),
                Span::from(skill.key.label()),
            ];
            if !skill.source_exists {
                spans.push(" ".into());
                spans.push(Span::from("missing source").dim());
            }
            let mut status = status_tokens(
                context.targets,
                context.selected_targets,
                skill,
                context.applied,
            );
            if !status.is_empty() {
                spans.push(" ".into());
                spans.append(&mut status);
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
    frame.render_stateful_widget(list, chunks[2], &mut state);
}

fn status_tokens(
    targets: &[AgentTarget],
    selected_targets: &HashSet<TargetKey>,
    skill: &ApplySkill,
    applied: &HashMap<(SkillKey, TargetKey), bool>,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for target in targets {
        if !selected_targets.contains(&target.key) {
            continue;
        }
        let applied_now = applied
            .get(&(skill.key.clone(), target.key.clone()))
            .copied()
            .unwrap_or(false);
        let marker = if applied_now { "*" } else { "." };
        let short = &target.short;
        let token = format!("{short}{marker}");
        let span = if applied_now {
            Span::from(token).style(theme::success_style())
        } else {
            Span::from(token).dim()
        };
        spans.push(span);
        spans.push(" ".into());
    }
    if !spans.is_empty() {
        spans.pop();
    }
    spans
}

fn render_summary(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    targets: &[AgentTarget],
    skills: &[ApplySkill],
    selected_targets: &HashSet<TargetKey>,
    selected_skills: &HashSet<SkillKey>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)].as_ref())
        .split(area);

    let header = Paragraph::new(Line::from(vec![
        Span::from("Confirm apply")
            .style(theme::accent_style())
            .bold(),
        "  ".into(),
        Span::from("Enter: apply  b: back  Esc/q: cancel").dim(),
    ]))
    .alignment(Alignment::Left);
    frame.render_widget(header, chunks[0]);

    let skill_labels = skills
        .iter()
        .filter(|skill| selected_skills.contains(&skill.key))
        .map(|skill| {
            let label = skill.key.label();
            format!("- {label}")
        })
        .collect::<Vec<_>>();
    let target_labels = targets
        .iter()
        .filter(|target| selected_targets.contains(&target.key))
        .map(|target| {
            let label = &target.label;
            let dir = target.base_dir.display();
            format!("- {label} -> {dir}")
        })
        .collect::<Vec<_>>();
    let mut lines = Vec::new();
    lines.push(Line::from(
        Span::from("Skills:").style(theme::accent_style()).bold(),
    ));
    if skill_labels.is_empty() {
        lines.push(Line::from("No skills selected".dim()));
    } else {
        lines.extend(skill_labels.into_iter().map(Line::from));
    }
    lines.push(Line::from(
        Span::from("Targets:").style(theme::accent_style()).bold(),
    ));
    if target_labels.is_empty() {
        lines.push(Line::from("No targets selected".dim()));
    } else {
        lines.extend(target_labels.into_iter().map(Line::from));
    }
    let body = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });
    frame.render_widget(body, chunks[1]);
}
