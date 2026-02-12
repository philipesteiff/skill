# End-to-End Usage and Logic Audit Report (`skill` workspace)

Date: 2026-02-11  
Audit type: Post-remediation follow-up  
Audited workspace: `/Users/philipesteiff/Projects/skill`

## Scope and Method
- Re-audited end-to-end behavior for `browse`, `sync`, and `apply` after recent remediation changes.
- Focused on command semantics, state transitions, and automation-relevant exit behavior.
- Used code inspection plus runtime probes in temporary `SKILLS_HOME` workspaces.
- Verified command surfaces:
  - `cargo run -- --help`
  - `cargo run -- browse --help`
  - `cargo run -- sync --help`
  - `cargo run -- apply --help`

## Findings Overview
| ID | Severity | Type | Title |
|---|---|---|---|
| H-10 | high | security/business-logic | Untrusted `metadata.version` can escape `SKILLS_HOME` install directory `[FIXED 2026-02-12]` |
| M-10 | medium | edge-case/logical-consistency | `apply` fails when `HOME` is unset even for project-only targets `[FIXED 2026-02-12]` |
| M-07 | medium | logical-consistency | `sync` (no-arg) fails globally when any configured source has empty selection `[FIXED 2026-02-12]` |
| M-08 | medium | logical-consistency | `apply` exits `0` when Git tracking updates fail `[FIXED 2026-02-12]` |
| M-09 | medium | business-logic | `apply` skip logic does not detect drift in managed target directories `[FIXED 2026-02-12]` |

## Detailed Findings

### H-10: Untrusted `metadata.version` can escape `SKILLS_HOME` install directory
Severity: `high`  
Type: `security/business-logic`
Status: `FIXED` (2026-02-12)

What is happening:
- Installer uses `resolved_version` directly as a filesystem path component for install output.
- `resolved_version` can come from source metadata (`SKILL.md` `metadata.version`) without path-safety validation.
- Absolute paths or traversal-like version values can redirect installed files outside `SKILLS_HOME`.

Why this is problematic:
- A source-controlled metadata field can control install destination path.
- This violates install directory isolation and can write files outside the managed skill home.
- Follow-on operations (sync/apply/uninstall) then operate on escaped paths recorded in `lock.json`.

Impact:
- Potential arbitrary write/delete behavior within user permissions during normal `browse`/`sync`.
- High risk for destructive side effects and unexpected filesystem mutation.

Evidence:
- Code paths:
  - `/Users/philipesteiff/Projects/skill/crates/skill-core/src/skills.rs:168`
  - `/Users/philipesteiff/Projects/skill/crates/skill-core/src/installer.rs:106`
  - `/Users/philipesteiff/Projects/skill/crates/skill-core/src/installer.rs:122`
  - `/Users/philipesteiff/Projects/skill/crates/skill-core/src/installer.rs:134`
- Runtime probe (2026-02-12):
  - Created local source with `metadata.version` set to an absolute temp path.
  - Ran `skill sync @evil-source`.
  - Command succeeded with status `0`.
  - Output showed install path outside `SKILLS_HOME`.
  - `lock.json` recorded external `install_dir`, and external `SKILL.md` was created.

Suggested fix:
- Decouple `resolved_version` display/storage from filesystem directory naming.
- Enforce safe install directory labels:
  - reject absolute paths,
  - reject `..` and path separators,
  - optionally normalize via safe slug/hash.
- Keep original version in lock metadata, but never use raw version string as a path join component.

Suggested tests:
- Add regression tests in `/Users/philipesteiff/Projects/skill/crates/skill-core/src/installer.rs` or `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/sync.rs`:
  - absolute `metadata.version` is rejected or sanitized,
  - traversal-like version values cannot escape `installed/<source>/<skill>/...`.

Fix implemented:
- `/Users/philipesteiff/Projects/skill/crates/skill-core/src/installer.rs`
  - Added strict install path label validation for `resolved_version`.
  - Installer now rejects unsafe version labels containing path/control semantics and returns an error.
  - Installation directory label defaults to commit short SHA when version is absent.
- `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/sync.rs`
  - Added `when_syncing_source_with_unsafe_version_should_fail_and_not_escape_install_root`.
- `/Users/philipesteiff/Projects/skill/crates/skill-core/src/installer.rs`
  - Added unit tests for version label validation behavior.

Verification:
- `cargo test -p skill-core installer`
- `cargo test -p skill-cli sync`
- `cargo test -p skill-core -p skill-features -p skill-cli`

---

### M-10: `apply` fails when `HOME` is unset even for project-only targets
Severity: `medium`  
Type: `edge-case/logical-consistency`
Status: `FIXED` (2026-02-12)

What is happening:
- `apply` always calls agent detection, which hard-fails if `HOME` is unset.
- This happens before target parsing and even when user explicitly requests project-only targets.

Why this is problematic:
- Project-only apply flows do not require global target paths.
- In minimal CI/systemd/container environments where `HOME` may be absent, valid project-only commands fail unnecessarily.

Impact:
- Hard failure in otherwise valid non-interactive apply flows.
- Extra environment coupling for automation.

Evidence:
- Code paths:
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:111`
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/agents.rs:255`
- Runtime probe (2026-02-12):
  - Ran:
    - `env -u HOME SKILLS_HOME=<tmp> skill apply --no-tui --targets claude:project --skills acme/echo-skill`
  - Result:
    - exit status `1`
    - stderr: `Error: HOME is not set`

Suggested fix:
- Make agent detection resilient to missing `HOME`:
  - always detect/build project targets,
  - include global targets only when `HOME` is present.
- Preserve current behavior for global targets when `HOME` exists.

Suggested tests:
- Add integration test in `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/apply.rs`:
  - unset `HOME`,
  - run project-only apply,
  - assert success path still works.

Fix implemented:
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/agents.rs`
  - Agent detection now tolerates missing `HOME` and still builds project targets.
  - Global targets are emitted only when `HOME` is available.
- `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/apply.rs`
  - Added `when_applying_project_target_with_home_unset_should_succeed`.
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/agents.rs`
  - Added unit test for missing-`HOME` target behavior.

Verification:
- `cargo test -p skill-features apply::agents`
- `cargo test -p skill-cli apply`
- `cargo test -p skill-core -p skill-features -p skill-cli`

### M-07: `sync` (no-arg) fails globally when any configured source has empty selection
Severity: `medium`  
Type: `logical-consistency`
Status: `FIXED` (2026-02-12)

What is happening:
- In no-arg mode (`skill sync`), all configured sources are iterated.
- For each source, sync fails hard when `desired.is_empty()` with:
  - `no skills selected; run skill browse to choose skills`.
- That per-source failure is aggregated and makes overall no-arg sync return non-zero, even when other sources sync successfully.

Why this is problematic:
- Empty selections are a normal reachable state:
  - a source can exist with default empty selection before first browse selection,
  - or become empty after uninstalling all skills.
- In those valid states, global sync behaves as a failure-oriented command instead of a best-effort refresh command.
- This creates noisy failures in automation pipelines that expect `skill sync` to refresh all active sources and ignore intentionally inactive ones.

Impact:
- Partial-success runs still return non-zero for a non-critical condition.
- CI/job runners can treat routine sync as failed even though all active sources were synced correctly.

Evidence:
- Code paths:
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/sync/mod.rs:107`
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/sync/mod.rs:109`
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/sync/mod.rs:49`
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/sync/mod.rs:81`
- Runtime probe (2026-02-11):
  - Configured two sources:
    - `source-a` with `selection = { mode: "list", skills: [] }`
    - `source-b` with `selection = { mode: "all" }`
  - `skill sync` behavior:
    - `source-b` installed successfully into `lock.json`
    - process exit status was `1` due to `source-a` empty-selection failure
    - stderr contained `sync failed for 1 source(s): @source-a`

Suggested fix:
- Treat empty selection as a non-error skip in no-arg sync-all mode:
  - record as `skipped source (no selected skills)` and continue.
- Preserve strict behavior for explicit per-source sync (`skill sync <SOURCE>`) if desired.
- Alternative: add explicit strict toggle (`--strict-empty-selection`) and default to non-failing skip in no-arg mode.

Suggested tests:
- Add integration coverage in `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/sync.rs`:
  - `when_syncing_without_source_and_a_source_has_empty_selection_should_skip_not_fail`
  - `when_syncing_explicit_source_with_empty_selection_should_error` (if strict single-source behavior is retained)

Fix implemented:
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/sync/mod.rs`
  - Added policy split for empty selections:
    - `skill sync` (no-arg): empty selection is skipped and does not fail run.
    - `skill sync <SOURCE>`: empty selection remains an explicit error.
- `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/sync.rs`
  - Added:
    - `when_syncing_without_source_and_one_source_has_empty_selection_should_skip_not_fail`
    - `when_syncing_explicit_source_with_empty_selection_should_error`

Verification:
- `cargo test -p skill-cli sync`
- `cargo test -p skill-core -p skill-features -p skill-cli`

---

### M-08: `apply` exits `0` when Git tracking updates fail
Severity: `medium`  
Type: `logical-consistency`
Status: `FIXED` (2026-02-12)

What is happening:
- `apply` records Git tracking failures in `results.tracking_failed` and prints a `Git Tracking Failed:` section.
- Final process failure is currently gated only by `results.failed` count.
- If content apply succeeds but tracking updates fail, command exits `0`.

Why this is problematic:
- Tracking preference is part of operator intent (`tracked` vs `not tracked`) and affects whether applied files are likely to be committed.
- A successful exit code can hide that `.git/info/exclude` updates failed and repository hygiene guarantees were not applied.

Impact:
- Automation and scripts can miss partial failures.
- Users can assume skills are excluded from Git when they are not, leading to accidental staging/commits.

Evidence:
- Code paths:
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:183`
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:267`
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:280`
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:712`
- Runtime probe (2026-02-11):
  - Ran `apply --no-tui` in a Git repo with non-writable `.git/info/exclude`.
  - Output included `Git Tracking Failed: ... write .../.git/info/exclude`.
  - Process exit status remained `0`.

Suggested fix:
- Treat non-empty `results.tracking_failed` as command failure (non-zero), at least in non-interactive/CLI mode.
- If softer behavior is desired for interactive flows, add explicit policy control (`--strict-tracking`).

Suggested tests:
- Add integration test in `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/apply.rs`:
  - make `.git/info/exclude` non-writable,
  - run apply,
  - assert non-zero status and tracking failure message.

Fix implemented:
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs`
  - Exit condition now fails command when either:
    - `results.failed` is non-empty, or
    - `results.tracking_failed` is non-empty.
- `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/apply.rs`
  - Added `when_git_tracking_update_fails_should_return_non_zero` (unix).

Verification:
- `cargo test -p skill-cli apply`
- `cargo test -p skill-core -p skill-features -p skill-cli`

---

### M-09: `apply` skip logic does not detect drift in managed target directories
Severity: `medium`  
Type: `business-logic`
Status: `FIXED` (2026-02-12)

What is happening:
- Refresh decision for existing managed targets uses only metadata in `applied.json` (`install_dir`, `resolved_commit`, `content_hash`).
- It does not validate destination directory contents before deciding `Skipped`.
- If a managed target directory is manually modified, re-apply can still skip and leave drifted files in place.

Why this is problematic:
- Managed applied copies can silently diverge from installed source contents.
- Reapplying does not restore expected content, breaking idempotence and predictability.

Impact:
- Stale or tampered skill content can persist despite successful `apply`.
- Operators may trust `Skipped` output even when target contents no longer match source.

Evidence:
- Code paths:
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:458`
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:620`
  - `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs:636`
- Runtime probe (2026-02-11):
  - Applied `src/alpha-skill` to project target.
  - Manually changed `project/.claude/skills/src__alpha-skill/SKILL.md`.
  - Re-ran same `apply --no-tui ...`.
  - Result was `Skipped`, status `0`, and tampered content remained unchanged.

Suggested fix:
- Add destination drift detection before skip decision:
  - compare destination content hash (or at minimum `SKILL.md` hash/metadata) against source install.
- If drift is detected, refresh destination and record as `Added`/`Refreshed` instead of `Skipped`.

Suggested tests:
- Add integration test in `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/apply.rs`:
  - apply skill,
  - tamper target file,
  - reapply,
  - assert destination restored to source content and action is not `Skipped`.

Fix implemented:
- `/Users/philipesteiff/Projects/skill/crates/skill-features/src/apply/mod.rs`
  - `entry_needs_refresh(...)` now validates destination content against source content (not metadata only).
  - Added directory/file comparison helpers to detect managed target drift and force refresh.
- `/Users/philipesteiff/Projects/skill/crates/skill-cli/tests/apply.rs`
  - Added `when_reapplying_after_managed_drift_should_refresh_content`.

Verification:
- `cargo test -p skill-cli apply`
- `cargo test -p skill-core -p skill-features -p skill-cli`

## Residual Risk
- `sync` no-arg behavior is now more capable than before; edge-policy interactions (empty selection, missing selection, broken source, and mixed states) should be fully codified in tests to prevent regressions.
