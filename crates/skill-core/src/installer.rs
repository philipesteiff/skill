use anyhow::{Result, anyhow};
use tempfile::tempdir;

use crate::git;
use crate::lockfile::{self, LockedSkill};
use crate::paths::Paths;
use crate::progress::{QueuedSkill, Reporter, SkillUpdate};
use crate::skills;
use crate::util::{copy_dir_recursive, remove_dir_if_exists, short_sha};

#[derive(Debug, Clone)]
pub struct InstallTarget {
    pub source_id: String,
    pub repo_url: String,
    pub path: String,
    pub commit: String,
    pub content_hash: Option<String>,
    pub expected_name: Option<String>,
    pub version: Option<String>,
    pub updated_at: Option<String>,
}

pub struct Installer<'a> {
    paths: &'a Paths,
}

impl<'a> Installer<'a> {
    pub fn new(paths: &'a Paths) -> Self {
        Self { paths }
    }

    pub fn install_targets(
        &self,
        targets: Vec<InstallTarget>,
        reporter: &mut Reporter,
    ) -> Result<()> {
        if targets.is_empty() {
            return Ok(());
        }
        let queue = targets
            .iter()
            .map(|target| QueuedSkill {
                name: target
                    .expected_name
                    .clone()
                    .unwrap_or_else(|| "skill".to_string()),
                version: target.version.clone(),
                requested: short_sha(&target.commit),
                source: target.source_id.clone(),
                commit: target.commit.clone(),
            })
            .collect();
        reporter.queue_skills(queue)?;

        for (index, target) in targets.iter().enumerate() {
            reporter.begin_skill(index)?;
            if let Err(err) = install_target(self.paths, reporter, target) {
                let _ = reporter.fail_active_skill(err.to_string());
                return Err(err);
            }
            reporter.finish_skill()?;
        }
        Ok(())
    }
}

fn install_target(paths: &Paths, reporter: &mut Reporter, target: &InstallTarget) -> Result<()> {
    if target.path.trim().is_empty() {
        return Err(anyhow!("missing skill path"));
    }

    reporter.step("Ensuring mirror cache")?;
    let mirror_path = paths.cache_repo_path(&target.repo_url);
    let repo_url_owned = target.repo_url.clone();
    let mirror_path_owned = mirror_path.clone();
    run_with_feedback(reporter, move || {
        git::ensure_mirror(&repo_url_owned, &mirror_path_owned)
    })?;

    reporter.step("Fetching commit (may take a while)")?;
    let mirror_path_owned = mirror_path.clone();
    let commit_owned = target.commit.clone();
    run_with_feedback(reporter, move || {
        git::fetch_commit(&mirror_path_owned, &commit_owned)
    })?;

    reporter.step("Extracting skill directory")?;
    let temp = tempdir()?;
    git::archive_path(&mirror_path, &target.commit, &target.path, temp.path())?;

    reporter.step("Validating SKILL.md")?;
    let skill_dir = temp.path().join(&target.path);
    let enforce_dir_match = target.path != ".";
    let spec = skills::read_skill_spec_with_options(&skill_dir, enforce_dir_match)?;

    if let Some(expected) = &target.expected_name
        && &spec.name != expected
    {
        return Err(anyhow!(
            "skill name mismatch: expected {}, found {}",
            expected,
            spec.name
        ));
    }

    let resolved_version = target.version.clone().or(spec.version.clone());
    reporter.update_active_skill(SkillUpdate {
        name: Some(spec.name.clone()),
        version: resolved_version.clone(),
    })?;

    let version_label = resolved_version
        .clone()
        .unwrap_or_else(|| short_sha(&target.commit));

    reporter.step("Copying files to skills home")?;
    let skill_root = paths
        .installed_dir()
        .join(&target.source_id)
        .join(&spec.name);
    remove_dir_if_exists(&skill_root)?;
    let install_dir = skill_root.join(&version_label);
    copy_dir_recursive(&skill_dir, &install_dir)?;

    reporter.step("Updating lockfile")?;
    let mut lock = lockfile::load(paths)?;
    let entry = LockedSkill {
        source_id: target.source_id.clone(),
        name: spec.name,
        resolved_version,
        resolved_commit: target.commit.clone(),
        content_hash: target.content_hash.clone(),
        path: target.path.clone(),
        install_dir: install_dir.to_string_lossy().to_string(),
        updated_at: target.updated_at.clone(),
    };
    lockfile::upsert(&mut lock, entry);
    lockfile::save(paths, &lock)?;

    reporter.step(format!(
        "Installed {} (commit {})",
        install_dir.display(),
        short_sha(&target.commit)
    ))?;
    Ok(())
}

fn run_with_feedback<F, T>(reporter: &mut Reporter, task: F) -> Result<T>
where
    F: Send + 'static + FnOnce() -> Result<T>,
    T: Send + 'static,
{
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = task();
        let _ = tx.send(result);
    });

    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => reporter.tick()?,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(anyhow!("operation canceled"));
            }
        }
    }
}
