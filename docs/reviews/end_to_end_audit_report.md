# End-to-End Usage and Logic Audit Report (`skill` workspace)

Date: 2026-02-11  
Audited workspace: `/Users/philipesteiff/Projects/skill`

## Scope and Method
- Reviewed all command flows from `/Users/philipesteiff/Projects/skill/crates/skill-cli/src/cli.rs` into `/Users/philipesteiff/Projects/skill/crates/skill-features` and `/Users/philipesteiff/Projects/skill/crates/skill-core`.
- Audited persistent state interactions for `config.json`, `lock.json`, `applied.json`, source index, source head, mirror cache, and installed skill trees.
- Compared behavior with docs and CLI help:
  - `/Users/philipesteiff/Projects/skill/README.md`
  - `/Users/philipesteiff/Projects/skill/playground/README.md`
  - `/Users/philipesteiff/Projects/skill/docs/project_main_plan.md`
  - `/Users/philipesteiff/Projects/skill/crates/skill-cli/src/cli.rs`
- Executed runtime verification:
  - `cargo test -p skill-core -p skill-features -p skill-cli`
  - `cargo clippy --all-targets --all-features`
  - Targeted scenario probes for collision, malformed search, apply semantics, and exit code behavior.

## Validation Summary
- `cargo test -p skill-core -p skill-features -p skill-cli`: passing.
- `cargo clippy --all-targets --all-features`: passing.
- Several high/medium issues are logic and flow-consistency defects not covered by current test assertions.

## Flow Inventory (Current Behavior)
| Command | Entry Point | Key State Read | Key State Write | Notes |
|---|---|---|---|---|
| `browse <source>` | `/Users/philipesteiff/Projects/skill/crates/skill-features/src/browse/mod.rs` | `config.json`, source index, `lock.json` | `config.json`, `lock.json`, installed files | Trusts source on first use and installs selected skills. |
| `browse` (installed view) | `/Users/philipesteiff/Projects/skill/crates/skill-features/src/browse/mod.rs` | `lock.json`, `config.json` | `lock.json`, `config.json` | TUI-only uninstall path. |
| `sync <source>` | `/Users/philipesteiff/Projects/skill/crates/skill-features/src/sync/mod.rs` | `config.json`, source index, `lock.json` | source index/head, `lock.json`, installed files | Installs/updates selected skills only. |
| `apply` | `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs` | `lock.json`, `applied.json`, environment/cwd | target directories, `applied.json`, git exclude (optional) | TUI desired-state mode. |
| `apply --no-tui` | `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs` | `lock.json`, `applied.json` | target directories, `applied.json` | Explicit apply/unapply list mode, not reconciliation mode. |

## Findings Overview
| ID | Severity | Type | Title |
|---|---|---|---|
| H-01 | high | business-logic | Mirror cache key collisions break source isolation |
| H-02 | high | business-logic | Duplicate skill names in one source collapse into one lock entry |
| H-03 | high | missing-flow | Uninstall flow does not clean applied targets or applied index |
| M-01 | medium | edge-case | Raw FTS query causes SQL failure for malformed search |
| M-02 | medium | usage-gap | `apply` help text implies reconciliation in CLI mode, but CLI mode is additive |
| M-03 | medium | logical-consistency | `apply` exits with status `0` even when operations fail [FIXED 2026-02-11] |
| M-04 | medium | ux-logic | Skipped apply actions are not shown in output [FIXED 2026-02-11] |
| M-05 | medium | ux-logic | Browse `select all` ignores active filter and selects hidden rows |
| M-06 | medium | missing-flow | Apply TUI can start with no selected target and Enter does nothing |
| L-01 | low | documentation/usage-gap | README usage is inconsistent with required sync argument |
| L-02 | low | documentation/usage-gap | Playground guide documents removed commands |

## Detailed Findings

### H-01: Mirror cache key collisions break source isolation
Severity: `high`  
Type: `business-logic`

What is happening:
- Mirror cache paths are derived from a lossy slug of URL text.
- Different URLs can normalize to the same slug and reuse the same bare mirror path.
- Existing mirror origin is not updated when the path already exists.

Why this is problematic:
- Distinct sources can be mapped to the same mirror, causing fetches against the wrong remote.
- A valid source can fail sync entirely, or worse, read data from a different source.

Impact:
- Hard failures and source cross-contamination in common multi-source setups.

Evidence:
- `/Users/philipesteiff/Projects/skill/crates/skill-core/src/paths.rs:65`
- `/Users/philipesteiff/Projects/skill/crates/skill-core/src/util.rs:11`
- `/Users/philipesteiff/Projects/skill/crates/skill-core/src/git.rs:93`
- Runtime probe:
  - Syncing `file://.../repo-a` succeeds.
  - Syncing `file://.../repo_a` then fails with `upload-pack: not our ref` because both map to one mirror directory.

Suggested fix:
- Replace slug-based mirror naming with a collision-resistant key (`sha256(normalized_url)`).
- Persist `mirror -> origin_url` metadata and verify it before fetch.
- If origin mismatch is detected, recreate or rewire the mirror.

Suggested tests:
- Add integration coverage in `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/sync.rs` with two colliding URLs and assert both sync independently.

---

### H-02: Duplicate skill names in one source collapse into one lock entry
Severity: `high`  
Type: `business-logic`

What is happening:
- Skills are uniquely keyed in lock/sync logic by `(source_id, name)` only.
- A source can contain multiple valid skills with the same `name` under different paths.
- Sync reports multiple installs, but lockfile keeps only one final entry.

Why this is problematic:
- Data loss at state layer: one installed skill overwrites another.
- Apply cannot target both because identity no longer represents path uniqueness.

Impact:
- Incorrect and incomplete installed set; confusing sync counts; potential silent overwrites.

Evidence:
- `/Users/philipesteiff/Projects/skill/crates/skill-core/src/lockfile.rs:44`
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/sync/mod.rs:55`
- `/Users/philipesteiff/Projects/skill/crates/skill-core/src/installer.rs:117`
- Runtime probe:
  - Source with `skills/echo-skill` and `other/echo-skill`.
  - Sync output: `installed 2`.
  - `lock.json`: only one `echo-skill` entry remains.

Suggested fix:
- Promote skill identity to include source path (or another canonical unique key), not just name.
- Update lockfile keys, sync lookup, apply key labels, and destination naming to preserve uniqueness.

Suggested tests:
- Add integration test ensuring two same-name skills from same source both persist in `lock.json` and remain independently applicable.

---

### H-03: Uninstall flow does not clean applied targets or applied index
Severity: `high`  
Type: `missing-flow`

What is happening:
- Installed-skill uninstall removes lockfile entry and install dir.
- It does not remove previously applied target directories.
- It does not prune `applied.json` entries for the removed skill.

Why this is problematic:
- User believes a skill is removed, but applied copies can remain active in agent folders.
- Future apply runs cannot reconcile removed skills that are no longer in lockfile selection set.

Impact:
- Stale behavior in user projects and long-lived state drift.

Evidence:
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/browse/mod.rs:192`
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:117`
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/applied_index.rs:56`

Suggested fix:
- During uninstall, enumerate matching applied entries and remove their target dirs (or explicitly prompt in TUI).
- Always remove corresponding `applied.json` entries when a skill is uninstalled.

Suggested tests:
- Add integration test:
  - install + apply skill
  - uninstall skill
  - assert target dir removed and `applied.json` no longer contains stale entries.

---

### M-01: Raw FTS query causes SQL failure for malformed search
Severity: `medium`  
Type: `edge-case`

What is happening:
- Search query is passed directly to SQLite FTS `MATCH`.
- Inputs with unmatched quotes or special syntax cause SQL logic errors.

Why this is problematic:
- User search input can crash the browse flow with backend SQL errors.

Impact:
- Unhandled user-input edge case with poor UX.

Evidence:
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/browse/mod.rs:266`
- `/Users/philipesteiff/Projects/skill/crates/skill-core/src/source_index.rs:127`
- Runtime probe:
  - `skill browse @src --search '"'`
  - Fails with `Error: unterminated string` / `SQL logic error`.

Suggested fix:
- Escape/sanitize user query before `MATCH`, or fallback to parameterized `LIKE` search on parse failure.
- Return user-safe error (`invalid search syntax`) instead of raw SQL parser output.

Suggested tests:
- Add unit or integration tests for problematic search tokens (`"`, `:`, `AND`, trailing operators).

---

### M-02: `apply` help text implies reconciliation in CLI mode, but CLI mode is additive
Severity: `medium`  
Type: `usage-gap`

What is happening:
- Help text states: unselected applied skills are removed in apply mode.
- CLI path (`--no-tui` / explicit flags) uses `ApplyOnly` and iterates selected skills only.

Why this is problematic:
- Automation users can assume reconciliation semantics and accidentally leave stale skills applied.

Impact:
- Logic surprise and inconsistent operator expectations.

Evidence:
- `/Users/philipesteiff/Projects/skill/crates/skill-cli/src/cli.rs:31`
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:154`
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:537`
- Runtime probe:
  - Apply all skills once.
  - Apply only one skill via `--no-tui --skills`.
  - Other previously applied skills remain.

Suggested fix:
- Clarify help text to distinguish:
  - TUI desired-state reconciliation mode
  - CLI explicit apply/unapply list mode
- Optional: add `--reconcile` flag for non-TUI parity.

Suggested tests:
- Integration tests for both explicit mode and reconciliation mode (if added).

---

### M-03: `apply` exits with status `0` even when operations fail
Severity: `medium`  
Type: `logical-consistency`
Status: `FIXED (2026-02-11)`

Fix implemented:
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs` now returns an error when action failures are present (`results.failed` is non-empty) after printing the `Failed:` section.
- `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/apply.rs` now asserts non-zero status for unmanaged-directory and symlink failure scenarios.

Verification:
- `cargo test -p skill-cli` passes with updated failure-status assertions.
- `cargo test -p skill-core -p skill-features -p skill-cli` passes.
- `cargo clippy --all-targets --all-features` passes.

What was happening:
- Failure rows were recorded and printed, but command still returned `Ok(())`.

Why this is problematic:
- CI/automation cannot detect failed apply operations by exit status.

Impact:
- Silent failures in scripts and deployment tooling.

Original evidence:
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:227`
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:267`
- Runtime probe:
  - Create unmanaged destination directory.
  - Apply reports failure row.
  - Process exit status remains `0`.

Suggested fix:
- Return non-zero when `results.failed` is non-empty.
- Optionally gate `tracking_failed` severity with a flag (`--strict-tracking`) but default should be explicit.

Suggested tests:
- Add integration assertions on process status for failure scenarios.

---

### M-04: Skipped apply actions are not shown in output
Severity: `medium`  
Type: `ux-logic`
Status: `FIXED (2026-02-11)`

Fix implemented:
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs` now renders a `Skipped:` section when `results.skipped` is non-empty, including per-action lines.
- `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/apply.rs` now includes `when_reapplying_should_report_skipped_action` to verify reapply output is explicit.

Verification:
- `cargo test -p skill-cli` passes with the new skipped-output integration assertion.
- `cargo test -p skill-core -p skill-features -p skill-cli` passes.
- `cargo clippy --all-targets --all-features` passes.

What was happening:
- `skipped` actions are collected but never printed.
- When all actions are skipped, user sees an empty results body.

Why this is problematic:
- Ambiguous no-op output makes behavior hard to verify.

Impact:
- Reduced debuggability and poor operator confidence.

Evidence:
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:192`
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:531`
- Runtime probe:
  - Apply same skill twice.
  - Second run prints header with no Added/Removed/Failed details.

Suggested fix:
- Add explicit `Skipped:` section with per-target rows.
- Optionally include skip reason (already applied / destination missing on unapply).

Suggested tests:
- Integration test that reapply output includes skipped count/details.

---

### M-05: Browse `select all` ignores active filter and selects hidden rows
Severity: `medium`  
Type: `ux-logic`

What is happening:
- `a` key selects every item in source list, not only currently filtered rows.

Why this is problematic:
- In filtered view, users can accidentally select/install/uninstall skills they cannot see.

Impact:
- Unexpected bulk actions, especially in large sources.

Evidence:
- `/Users/philipesteiff/Projects/skill/crates/skill-core/src/ui/browse.rs:344`

Suggested fix:
- Make `a` operate on visible (`filtered`) rows.
- If full-list selection is still needed, map it to a separate explicit key and label.

Suggested tests:
- Add unit tests in browse UI module:
  - filtered list + `a` selects only filtered paths.

---

### M-06: Apply TUI can start with no selected target and Enter does nothing
Severity: `medium`  
Type: `missing-flow`

What is happening:
- If no target is `default_selected`, `selected_target` is `None`.
- Highlighted row exists, but Enter is ignored until user changes selection with movement keys.

Why this is problematic:
- First-step UX dead-end in undetected environments.

Impact:
- Incomplete/fragile flow for first-time or non-detected setups.

Evidence:
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/ui.rs:94`
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/ui.rs:155`

Suggested fix:
- Initialize `selected_target` to highlighted row when defaults are absent.
- Keep current detection-based defaults as priority when present.

Suggested tests:
- Add UI-state unit test covering "no default selection" startup behavior.

---

### L-01: README usage is inconsistent with required `sync` argument
Severity: `low`  
Type: `documentation/usage-gap`

What is happening:
- README shows `skill sync` with no argument.
- CLI requires `skill sync <SOURCE>`.

Why this is problematic:
- New users hit immediate command errors from official docs.

Impact:
- Onboarding friction.

Evidence:
- `/Users/philipesteiff/Projects/skill/README.md:56`
- `/Users/philipesteiff/Projects/skill/crates/skill-cli/src/cli.rs:24`

Suggested fix:
- Update README examples to include source forms (`@source-id` and repo shorthand).

Suggested tests:
- Doc sanity check in CI for command snippets against `--help` usage (scripted smoke check).

---

### L-02: Playground guide documents removed commands
Severity: `low`  
Type: `documentation/usage-gap`

What is happening:
- Playground guide still references `add-registry`, `search`, and `install`.
- Current CLI only exposes `browse`, `sync`, and `apply`.

Why this is problematic:
- Developer onboarding and local validation playbook are broken.

Impact:
- Wasted setup/debug time for contributors.

Evidence:
- `/Users/philipesteiff/Projects/skill/playground/README.md:19`
- Runtime probe:
  - `cargo run -- add-registry foo`
  - Fails with `unrecognized subcommand 'add-registry'`.

Suggested fix:
- Rewrite playground usage around current command set and supported local source workflow.

Suggested tests:
- Add docs smoke script to run every documented command in playground README.

## Testing Gaps and Recommended Additions
- Add collision tests for mirror cache key derivation and independent source sync.
- Add duplicate-name-in-source tests covering lockfile identity behavior.
- Add uninstall-applied cleanup tests spanning `browse` uninstall + apply state.
- Add robust search-input tests for browse FTS fallback behavior.
- Add exit-code tests for `apply` failure and tracking-failure modes.
- Add output assertions for skipped action reporting.
- Add browse selection behavior tests for filtered select-all semantics.
- Add apply UI initialization tests for no-default-selection case.

## Quick Wins (Low Effort, High Value)
- Fix README `sync` example and playground command set.
- Add `Skipped:` output section in apply results.
- Clarify `apply` help text to separate TUI reconciliation vs CLI explicit mode.
- Return non-zero exit status when `apply` has action failures.

## Residual Risk
- Private repo auth and network error-path UX were not fully exercised against external remotes due local/offline constraints.
- Interactive key-path behavior was validated primarily by code inspection where deterministic non-TTY probes are unavailable.

## Appendix: Commands Executed
- `cargo test -p skill-core -p skill-features -p skill-cli`
- `cargo clippy --all-targets --all-features`
- `cargo run -- --help`
- `cargo run -- browse --help`
- `cargo run -- sync --help`
- `cargo run -- apply --help`
- Targeted probes:
  - collision reproduction for cache key slugs
  - duplicate-name source sync
  - malformed browse search input
  - apply CLI semantics and output/exit behavior
  - stale docs command verification
