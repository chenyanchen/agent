use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::Error;

const MAX_CATALOG_CHARS: usize = 8_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub allow_implicit_invocation: bool,
}

#[derive(Debug, Default)]
pub(crate) struct SkillCatalog {
    pub skills: Vec<SkillInfo>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
}

#[derive(Default, Deserialize)]
struct OpenAiYaml {
    #[serde(default)]
    policy: SkillPolicy,
}

#[derive(Default, Deserialize)]
struct SkillPolicy {
    #[serde(default = "default_true")]
    allow_implicit_invocation: bool,
}

fn default_true() -> bool {
    true
}

impl SkillCatalog {
    pub fn scan(workdir: &Path, user_skills_dir: &Path) -> Self {
        let mut catalog = Self::default();
        let mut names = HashSet::new();
        for root in repository_skill_roots(workdir)
            .into_iter()
            .chain(std::iter::once(user_skills_dir.to_path_buf()))
        {
            let Ok(entries) = fs::read_dir(&root) else {
                continue;
            };
            let mut found = Vec::new();
            for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
                let path = entry.path().join("SKILL.md");
                if !path.is_file() {
                    continue;
                }
                match read_skill(&path) {
                    Ok(skill) => found.push(skill),
                    Err(error) => catalog.warnings.push(error.to_string()),
                }
            }
            found.sort_by(|left, right| left.name.cmp(&right.name));
            catalog.skills.extend(
                found
                    .into_iter()
                    .filter(|skill| names.insert(skill.name.clone())),
            );
        }
        let omitted = catalog
            .skills
            .len()
            .saturating_sub(catalog.prompt_lines().len());
        if omitted > 0 {
            catalog.warnings.push(format!(
                "skill catalog exceeded {MAX_CATALOG_CHARS} characters; omitted {omitted} lowest-precedence entries from implicit selection"
            ));
        }
        catalog
    }

    pub fn prompt(&self) -> String {
        self.prompt_lines().join("\n")
    }

    fn prompt_lines(&self) -> Vec<String> {
        let format_lines = |description_limit: Option<usize>| {
            self.skills
                .iter()
                .map(|skill| {
                    let description = match description_limit {
                        Some(limit) if skill.description.chars().count() > limit => {
                            skill
                                .description
                                .chars()
                                .take(limit - 3)
                                .collect::<String>()
                                + "..."
                        }
                        _ => skill.description.clone(),
                    };
                    format!(
                        "- ${}: {} [{}] ({})",
                        skill.name,
                        description,
                        if skill.allow_implicit_invocation {
                            "implicit allowed"
                        } else {
                            "explicit only"
                        },
                        skill.path.display()
                    )
                })
                .collect::<Vec<_>>()
        };
        let mut lines = format_lines(None);
        if lines.join("\n").chars().count() > MAX_CATALOG_CHARS {
            lines = format_lines(Some(160));
        }
        while lines.join("\n").chars().count() > MAX_CATALOG_CHARS {
            lines.pop();
        }
        lines
    }

    pub fn validate_explicit(&self, input: &str) -> Result<(), Error> {
        for name in explicit_names(input) {
            if !self.skills.iter().any(|skill| skill.name == name) {
                return Err(Error::Other(format!(
                    "explicit skill `${name}` was not found or is invalid"
                )));
            }
        }
        Ok(())
    }
}

fn repository_skill_roots(workdir: &Path) -> Vec<PathBuf> {
    let ancestors = workdir.ancestors().collect::<Vec<_>>();
    let git_index = ancestors.iter().position(|path| path.join(".git").exists());
    let last = git_index.unwrap_or(0);
    ancestors[..=last]
        .iter()
        .map(|path| path.join(".agents/skills"))
        .collect()
}

fn read_skill(path: &Path) -> Result<SkillInfo, Error> {
    let text = fs::read_to_string(path)
        .map_err(|error| Error::Other(format!("{}: {error}", path.display())))?;
    let yaml = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---"))
        .map(|(yaml, _)| yaml)
        .ok_or_else(|| Error::Other(format!("{}: missing YAML frontmatter", path.display())))?;
    let metadata: Frontmatter = serde_yaml::from_str(yaml)
        .map_err(|error| Error::Other(format!("{}: invalid metadata: {error}", path.display())))?;
    if metadata.name.trim().is_empty() || metadata.description.trim().is_empty() {
        return Err(Error::Other(format!(
            "{}: name and description must be non-empty",
            path.display()
        )));
    }
    let policy_path = path.parent().unwrap().join("agents/openai.yaml");
    let allow_implicit_invocation = match fs::read_to_string(&policy_path) {
        Ok(text) => {
            serde_yaml::from_str::<OpenAiYaml>(&text)
                .map_err(|error| {
                    Error::Other(format!(
                        "{}: invalid policy: {error}",
                        policy_path.display()
                    ))
                })?
                .policy
                .allow_implicit_invocation
        }
        Err(_) => true,
    };
    Ok(SkillInfo {
        name: metadata.name,
        description: metadata.description,
        path: path.to_path_buf(),
        allow_implicit_invocation,
    })
}

fn explicit_names(input: &str) -> Vec<&str> {
    input
        .split_whitespace()
        .filter_map(|word| word.strip_prefix('$'))
        .map(|name| {
            name.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
        })
        .filter(|name| !name.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(root: &Path, dir: &str, name: &str, description: &str) {
        let path = root.join(dir);
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: >\n  {description}\n---\nbody"),
        )
        .unwrap();
    }

    #[test]
    fn nearest_repository_skill_wins_and_policy_can_forbid_implicit_use() {
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir(repo.path().join(".git")).unwrap();
        let workdir = repo.path().join("a/b");
        fs::create_dir_all(&workdir).unwrap();
        write_skill(&repo.path().join(".agents/skills"), "same", "same", "root");
        write_skill(&workdir.join(".agents/skills"), "same", "same", "nearest");
        fs::create_dir_all(workdir.join(".agents/skills/same/agents")).unwrap();
        fs::write(
            workdir.join(".agents/skills/same/agents/openai.yaml"),
            "policy:\n  allow_implicit_invocation: false\n",
        )
        .unwrap();

        let catalog = SkillCatalog::scan(&workdir, &repo.path().join("user"));
        assert_eq!(catalog.skills.len(), 1);
        assert_eq!(catalog.skills[0].description.trim(), "nearest");
        assert!(!catalog.skills[0].allow_implicit_invocation);
        assert!(catalog.validate_explicit("use $same").is_ok());
        assert!(catalog.validate_explicit("use $missing").is_err());
    }
}
