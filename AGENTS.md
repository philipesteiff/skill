# Skill project

These instructions apply anywhere Rust code lives in this repository (crates, tools, libraries, CLIs, TUIs, and tests).

The goal is to write **clean, idiomatic, well-tested Rust**, with consistent formatting, Clippy compliance, and
high-quality TUI code where applicable.

---

## Core coding conventions

* Prefer idiomatic, readable Rust over cleverness.
* Keep modules small and cohesive; avoid generic “utils” modules unless clearly justified.
* Prefer expressive types (structs/enums) over tuples or parallel collections.
* Prefer iterators over manual indexing when it improves clarity.
* Use early returns and the `?` operator consistently for error handling.
* Keep public APIs minimal and intentional.
* Avoid exposing internals “just in case”.

---

## Writing Plans

I'm using the writing-plans to create the implementation plan.

You are writing for an engineer who has:
- zero context about our codebase or domain
- questionable taste
- weak test design instincts
  …but they are still a skilled developer. Your plan must be safe, explicit, and hard to misread.

### Output format (strict)
Use this exact structure and keep it skimmable:

1) Purpose & Success Criteria
2) Scope (In / Out)
3) Assumptions & Constraints
4) System Overview (current → target)
5) Milestones (checklist)
6) Testing Strategy (unit/integration/e2e) + commands
7) Observability & Debugging (logs, metrics, tracing) if relevant
8) Docs & UX copy updates
9) Rollout / Compatibility / Migration (if relevant)
10) Open Questions (tagged as BLOCKER or NON-BLOCKER)

### 1) Purpose & Success Criteria
- Start with "why" in 2–4 sentences.
- Define measurable success criteria (what must work, how we verify it).
- Include a small “Demo script” (3–6 steps) describing what someone should be able to do at the end.

### 2) Scope (In / Out)
- List must-haves vs nice-to-haves.
- Explicitly mark out-of-scope items to prevent gold-plating (YAGNI).

### 3) Assumptions & Constraints
- Include invariants (e.g. “must be backwards compatible”, “no new deps”, “CLI must remain stable”).
- If you lack info, do not guess silently. Add an assumption + how to validate it.

### 4) System Overview (current → target)
- Explain the current architecture at a high level (what exists today).
- Describe the target architecture and the smallest change that moves us there.
- Call out key dependencies and risk areas.

### 5) Milestones (the core of the plan)
Write iterative milestones that are independently valuable. For each milestone:

**Milestone template (required)**
- Goal (one sentence)
- Files to touch (explicit paths; create new files only if justified)
- Implementation steps (3–8 bullets, ordered)
- Tests to add (what + where + why)
- How to verify locally (exact commands)
- Definition of Done (checkboxes)
- Commit(s) (suggest 1–3 commit boundaries with commit messages)

Milestone rules:
- Prefer small milestones over one giant “implement everything”.
- Show dependencies between milestones.
- Default to TDD: tests first when practical.
- DRY: avoid repeating the same instruction across milestones; factor shared steps into one place and reference it.

### 6) Testing Strategy
- Specify what to test at each layer (unit/integration/e2e), plus minimal happy-path coverage.
- Include how to run tests + any fixtures/mocks needed.
- If tests are hard, propose a tiny refactor milestone to make them testable.

### 7) Observability & Debugging (if relevant)
- What logs/metrics/traces to add, with sample log lines or fields.
- How to diagnose failures quickly.

### 8) Docs & UX copy updates
- List docs files / README sections / CLI help text to update.
- Include user-facing messages/error strings where relevant.

### 9) Rollout / Compatibility / Migration (if relevant)
- Backwards compatibility concerns.
- Feature flags, migrations, or phased rollout steps.

### 10) Open Questions
- Only ask questions that materially change the plan.
- Tag each as:
  - BLOCKER: cannot proceed without answer
  - NON-BLOCKER: can proceed with a safe default
- For NON-BLOCKER, propose a default decision.

### Quality bar checklist (must satisfy)
Before finalizing, verify:
- A junior dev could execute it without asking you what files to edit.
- Every milestone has a clear DoD and test/verify steps.
- “Happy path” is implementable early; edge cases can come later.
- No hidden assumptions.
- No timeline estimates.

---

## Formatting, linting, and `just`

* Always run formatting after Rust changes:

    * `just fmt`
    * Do **not** ask for approval to run formatting.
* Fix lint issues before finalizing:

    * Prefer scoped linting: `just fix -p <crate>`
    * Only run workspace-wide linting if you changed shared crates.
* Apply these Clippy rules whenever they improve clarity and do not conflict with local conventions:

    * Collapsible if
      [https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_if](https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_if)
    * Uninlined format arguments
      [https://rust-lang.github.io/rust-clippy/master/index.html#uninlined_format_args](https://rust-lang.github.io/rust-clippy/master/index.html#uninlined_format_args)
    * Method references over redundant closures
      [https://rust-lang.github.io/rust-clippy/master/index.html#redundant_closure_for_method_calls](https://rust-lang.github.io/rust-clippy/master/index.html#redundant_closure_for_method_calls)
* `format!`, `println!`, `panic!`, `anyhow!`, etc.:

    * Inline variables directly in `{}` whenever possible
      Example: `format!("Hello {name}")` instead of `format!("Hello {}", name)`

---

## Tests

* Always run the most specific tests first:

    * `cargo test -p <crate>`
* If you changed shared crates, core abstractions, or cross-cutting behavior:

    * Also run `cargo test --all-features`
    * Ask the user before running a full workspace test suite if it is expected to be slow.
* Targeted crate tests and individual tests can be run without asking.

### Test assertions

* Prefer asserting equality of **entire objects** rather than field-by-field checks.
* Use `pretty_assertions::assert_eq!` (and `assert_ne!`) when it improves diff readability.
* Avoid mutating global process state in tests:

    * Do not globally set environment variables unless unavoidable.
    * Prefer passing configuration explicitly.
* Test function names must follow the pattern `when...should...`.

---

## Documentation expectations

* When adding or modifying a public API (functions, types, CLI flags, config, file formats):

    * Update relevant documentation (`README.md`, `docs/`, crate-level docs, or item docs).
* Keep examples accurate and consistent with the codebase.

## TUI requirement

* This project requires a modern terminal UI for user feedback and selection.
* Use `ratatui` for TUI components and keep the TUI responsive and informative across workflows.

---

## Snapshot tests (if used)

If this repository uses `insta` snapshot tests and output changes intentionally:

* Generate updated snapshots:

    * `cargo test -p <crate>`
* Review pending snapshots:

    * `cargo insta pending-snapshots -p <crate>`
* Inspect a specific snapshot:

    * `cargo insta show -p <crate> path/to/file.snap.new`
* Accept snapshots only when intended:

    * `cargo insta accept -p <crate>`
* If missing:

    * `cargo install cargo-insta`

---

## TUI conventions (ratatui)

These apply to any terminal UI code using `ratatui`.

### Styling

* Prefer concise helpers from `ratatui::style::Stylize`:

    * Plain spans: `"text".into()`
    * Styled spans: `"text".red()`, `"text".green()`, `"text".magenta()`, `"text".dim()`, etc.
* Prefer these helpers over manually constructing `Style` and `Span::styled`, unless the style is computed at runtime.
* Avoid hardcoded white:

    * Do **not** use `.white()`
    * Prefer default foreground color.
* Chaining is encouraged when readable:

    * Example: `url.cyan().underlined()`
* Follow file-local conventions and avoid refactors that are purely stylistic.
* Prefer forms that stay on one line after `rustfmt` when possible.

### Building spans and lines

* Prefer `"text".into()` when the target type is obvious.
* Use `vec![…].into()` to build a `Line` when inference is clear.
* If inference is ambiguous, use `Line::from(spans)` or `Span::from(text)`.
* Do not introduce extra type annotations solely to satisfy `.into()`.

### Text wrapping

* Always use `textwrap::wrap` for wrapping plain strings.
* For wrapping styled `Line`/`Span` content:

    * Prefer project-provided helpers.
    * If none exist, introduce a small shared helper instead of ad-hoc logic.
* For indentation:

    * Prefer `textwrap::Options` (`initial_indent`, `subsequent_indent`) over custom indentation code.

---

## Tooling prerequisites

* If the repository depends on external tools (for example `just`, `rg`, `cargo-insta`):

    * Ensure they are installed before running dependent commands.
* Prefer the provided `just` tasks over ad-hoc Cargo commands when available.

---

## Change discipline

* Keep diffs minimal and focused.
* Avoid unrelated formatting or refactors.
* Make changes in small, testable steps.
* For larger refactors:

    * Keep changes logically separated.
    * Ensure tests remain green throughout the process.
