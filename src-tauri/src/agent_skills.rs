use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

const SKILLS: [&str; 3] = ["pulsedeck-control", "pulsedeck-library", "djing-art"];
const LEGACY: [&str; 7] = [
    "pulsedeck-track-inspection",
    "pulsedeck-set-preparation",
    "pulsedeck-prepare-next-track",
    "pulsedeck-transition",
    "pulsedeck-session-dj",
    "pulsedeck-recording-and-export",
    "pulsedeck-setup-and-troubleshooting",
];
static INSTALL: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Agent {
    Codex,
    Claude,
}

#[derive(Serialize)]
pub struct Report {
    agent: Agent,
    installed: bool,
    destination: PathBuf,
    backup: Option<PathBuf>,
    error: Option<String>,
}

pub fn welcome(data_dir: &Path, dismiss: bool) -> Result<bool, String> {
    let path = data_dir.join("agent-skills-welcome-v1");
    if dismiss {
        match fs::File::create_new(&path) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("Could not save welcome preference: {e}")),
        }
    }
    path.try_exists()
        .map(|exists| !exists)
        .map_err(|e| e.to_string())
}

fn agent_home(agent: Agent, home: &Path) -> Result<PathBuf, String> {
    let (variable, folder) = match agent {
        Agent::Codex => ("CODEX_HOME", ".codex"),
        Agent::Claude => ("CLAUDE_CONFIG_DIR", ".claude"),
    };
    match std::env::var_os(variable).filter(|v| !v.is_empty()) {
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err(format!("{variable} must be an absolute folder path."));
            }
            Ok(path)
        }
        None => Ok(home.join(folder)),
    }
}

pub fn install(source: &Path, home: &Path, agents: Vec<Agent>) -> Result<Vec<Report>, String> {
    if agents.is_empty() || agents.len() > 2 || (agents.len() == 2 && agents[0] == agents[1]) {
        return Err("Choose Codex, Claude Code, or both once.".into());
    }
    for name in SKILLS {
        if !source.join(name).join("SKILL.md").is_file() {
            return Err(
                "The bundled DJ skills are incomplete. Reinstall PulseDeck from the release ZIP."
                    .into(),
            );
        }
    }
    let _lock = INSTALL
        .lock()
        .map_err(|_| "Skill setup is unavailable. Restart PulseDeck and try again.")?;
    Ok(agents
        .into_iter()
        .map(|agent| {
            let target = agent_home(agent, home);
            let destination = target
                .as_ref()
                .map(|p| p.join("skills"))
                .unwrap_or_default();
            match target.and_then(|target| install_one(source, &target)) {
                Ok(backup) => Report {
                    agent,
                    installed: true,
                    destination,
                    backup,
                    error: None,
                },
                Err(error) => Report {
                    agent,
                    installed: false,
                    destination,
                    backup: None,
                    error: Some(error),
                },
            }
        })
        .collect())
}

fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    let kind = fs::symlink_metadata(source)?.file_type();
    if kind.is_dir() {
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if kind.is_file() {
        fs::copy(source, destination)?;
    } else {
        return Err(io::Error::other(
            "Bundled skills contain an unsupported link or file.",
        ));
    }
    Ok(())
}

// Backups live outside skill discovery; publish only after the complete copy is staged.
fn publish(stage: &Path, destination: &Path, backup: &Path) -> Result<bool, String> {
    let mut saved = Vec::new();
    let mut published = Vec::new();
    let result = (|| -> io::Result<()> {
        for name in SKILLS.into_iter().chain(LEGACY) {
            match fs::symlink_metadata(destination.join(name)) {
                Ok(_) => {
                    fs::create_dir_all(backup)?;
                    fs::rename(destination.join(name), backup.join(name))?;
                    saved.push(name);
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        for name in SKILLS {
            fs::rename(stage.join(name), destination.join(name))?;
            published.push(name);
        }
        Ok(())
    })();
    if let Err(error) = result {
        let mut failed = Vec::new();
        for name in published {
            if fs::remove_dir_all(destination.join(name)).is_err() {
                failed.push(name);
            }
        }
        for name in &saved {
            if fs::rename(backup.join(name), destination.join(name)).is_err() {
                failed.push(name);
            }
        }
        return Err(if failed.is_empty() {
            format!("Could not install skills: {error}. Previous skills were preserved.")
        } else {
            format!(
                "Could not install skills: {error}. Recover saved skills from {}: {}.",
                backup.display(),
                failed.join(", ")
            )
        });
    }
    Ok(!saved.is_empty())
}

fn install_one(source: &Path, home: &Path) -> Result<Option<PathBuf>, String> {
    let destination = home.join("skills");
    if fs::symlink_metadata(&destination).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!(
            "{} is a link. Install the bundled skills manually into its intended folder.",
            destination.display()
        ));
    }
    fs::create_dir_all(&destination)
        .map_err(|e| format!("Could not access {}: {e}", destination.display()))?;
    let id = format!("{:016x}", rand::random::<u64>());
    let stage = home.join(format!(".pulsedeck-skills-{id}"));
    let backup = home.join("pulsedeck-skill-backups").join(id);
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = (|| {
        for name in SKILLS {
            copy_tree(&source.join(name), &stage.join(name)).map_err(|e| e.to_string())?;
        }
        let backed_up = publish(&stage, &destination, &backup)?;
        Ok(backed_up.then_some(backup))
    })();
    let _ = fs::remove_dir_all(stage);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "pulsedeck-agent-skills-{:016x}",
                rand::random::<u64>()
            ));
            fs::create_dir(&root).unwrap();
            for name in SKILLS {
                fs::create_dir_all(root.join("source").join(name).join("references")).unwrap();
                fs::write(root.join("source").join(name).join("SKILL.md"), "bundled").unwrap();
                fs::write(
                    root.join("source").join(name).join("references/guide.md"),
                    "guide",
                )
                .unwrap();
            }
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn both_agent_folders_install_update_and_back_up_without_touching_unrelated_skills() {
        let f = Fixture::new();
        for folder in [".codex", ".claude"] {
            let home = f.0.join(folder);
            fs::create_dir_all(home.join("skills/unrelated")).unwrap();
            fs::write(home.join("skills/unrelated/SKILL.md"), "keep").unwrap();
            assert!(install_one(&f.0.join("source"), &home).unwrap().is_none());
            fs::write(home.join("skills/pulsedeck-control/SKILL.md"), "custom").unwrap();
            fs::create_dir(home.join("skills").join(LEGACY[0])).unwrap();
            let backup = install_one(&f.0.join("source"), &home).unwrap().unwrap();
            assert_eq!(
                fs::read_to_string(backup.join("pulsedeck-control/SKILL.md")).unwrap(),
                "custom"
            );
            assert!(backup.join(LEGACY[0]).is_dir());
            assert!(!home.join("skills").join(LEGACY[0]).exists());
            assert_eq!(
                fs::read_to_string(home.join("skills/unrelated/SKILL.md")).unwrap(),
                "keep"
            );
            assert_eq!(
                fs::read_to_string(home.join("skills/djing-art/references/guide.md")).unwrap(),
                "guide"
            );
        }
    }

    #[test]
    fn failed_publication_rolls_back_existing_skills() {
        let f = Fixture::new();
        let home = f.0.join("agent");
        install_one(&f.0.join("source"), &home).unwrap();
        fs::write(home.join("skills/pulsedeck-control/SKILL.md"), "custom").unwrap();
        let stage = f.0.join("incomplete-stage");
        fs::create_dir_all(stage.join(SKILLS[0])).unwrap();
        assert!(publish(&stage, &home.join("skills"), &f.0.join("backup")).is_err());
        assert_eq!(
            fs::read_to_string(home.join("skills/pulsedeck-control/SKILL.md")).unwrap(),
            "custom"
        );
        for name in SKILLS {
            assert!(home.join("skills").join(name).join("SKILL.md").is_file());
        }
    }

    #[test]
    fn incomplete_source_and_invalid_selection_do_not_create_an_agent_folder() {
        let f = Fixture::new();
        fs::remove_file(f.0.join("source/djing-art/SKILL.md")).unwrap();
        assert!(install(&f.0.join("source"), &f.0.join("home"), vec![Agent::Codex]).is_err());
        assert!(install(&f.0.join("source"), &f.0.join("home"), vec![]).is_err());
        assert!(install(
            &f.0.join("source"),
            &f.0.join("home"),
            vec![Agent::Claude, Agent::Claude]
        )
        .is_err());
        assert!(!f.0.join("home").exists());
    }

    #[test]
    fn welcome_is_once_per_profile_and_dismissal_is_idempotent() {
        let f = Fixture::new();
        assert!(welcome(&f.0, false).unwrap());
        assert!(!welcome(&f.0, true).unwrap());
        assert!(!welcome(&f.0, true).unwrap());
        assert!(!welcome(&f.0, false).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn linked_skill_roots_are_refused_without_writing_the_target() {
        let f = Fixture::new();
        let home = f.0.join("agent");
        fs::create_dir(&home).unwrap();
        fs::create_dir(f.0.join("outside")).unwrap();
        std::os::unix::fs::symlink(f.0.join("outside"), home.join("skills")).unwrap();
        assert!(install_one(&f.0.join("source"), &home).is_err());
        assert_eq!(fs::read_dir(f.0.join("outside")).unwrap().count(), 0);
    }
}
