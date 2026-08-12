use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::Error;

const SKILL_METADATA_CONTEXT_PERCENT: usize = 2;
const APPROX_BYTES_PER_TOKEN: usize = 4;
const MAX_DESCRIPTION_CHARS: usize = 1_024;

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
    prompt: String,
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
    pub fn scan(workdir: &Path, user_skills_dir: &Path, context_window: u32) -> Self {
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
        let (lines, shortened, omitted) = prompt_lines(&catalog.skills, context_window);
        catalog.prompt = lines.join("\n");
        if omitted > 0 {
            catalog.warnings.push(format!(
                "exceeded the 2% skills context budget; removed all descriptions and omitted {omitted} lowest-precedence entries from implicit selection"
            ));
        } else if shortened {
            catalog.warnings.push(
                "skill descriptions were shortened to fit the 2% skills context budget; every skill remains available"
                    .into(),
            );
        }
        catalog
    }

    pub fn prompt(&self) -> String {
        self.prompt.clone()
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

fn prompt_lines(skills: &[SkillInfo], context_window: u32) -> (Vec<String>, bool, usize) {
    let budget = (context_window as usize)
        .saturating_mul(SKILL_METADATA_CONTEXT_PERCENT)
        .saturating_div(100)
        .max(1);
    let descriptions = skills
        .iter()
        .map(|skill| {
            let mut chars = skill.description.chars().collect::<Vec<_>>();
            if chars.len() > MAX_DESCRIPTION_CHARS {
                chars.truncate(MAX_DESCRIPTION_CHARS - 3);
                chars.extend(['.', '.', '.']);
            }
            chars
        })
        .collect::<Vec<_>>();
    let minimum = skills
        .iter()
        .map(|skill| format_skill(skill, ""))
        .collect::<Vec<_>>();
    let minimum_cost = lines_cost(&minimum);

    if minimum_cost > budget {
        let mut used = 0usize;
        let mut included = Vec::new();
        for line in minimum {
            let cost = line_cost(&line);
            if used.saturating_add(cost) <= budget {
                used = used.saturating_add(cost);
                included.push(line);
            }
        }
        let omitted = skills.len().saturating_sub(included.len());
        return (included, true, omitted);
    }

    let mut allocated = vec![0usize; skills.len()];
    let mut costs = minimum
        .iter()
        .map(|line| line_cost(line))
        .collect::<Vec<_>>();
    let mut remaining = budget.saturating_sub(minimum_cost);
    loop {
        let mut changed = false;
        for index in 0..skills.len() {
            if allocated[index] == descriptions[index].len() {
                continue;
            }
            let next = allocated[index] + 1;
            let description = descriptions[index][..next].iter().collect::<String>();
            let next_cost = line_cost(&format_skill(&skills[index], &description));
            let delta = next_cost.saturating_sub(costs[index]);
            if delta <= remaining {
                allocated[index] = next;
                costs[index] = next_cost;
                remaining = remaining.saturating_sub(delta);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let shortened = allocated
        .iter()
        .zip(&descriptions)
        .any(|(allocated, description)| *allocated < description.len());
    let lines = skills
        .iter()
        .zip(descriptions)
        .zip(allocated)
        .map(|((skill, description), allocated)| {
            format_skill(skill, &description[..allocated].iter().collect::<String>())
        })
        .collect();
    (lines, shortened, 0)
}

fn format_skill(skill: &SkillInfo, description: &str) -> String {
    format!(
        "- ${}: {}[{}] ({})",
        skill.name,
        if description.is_empty() {
            String::new()
        } else {
            format!("{description} ")
        },
        if skill.allow_implicit_invocation {
            "implicit allowed"
        } else {
            "explicit only"
        },
        skill.path.display()
    )
}

fn line_cost(line: &str) -> usize {
    line.len()
        .saturating_add(1)
        .saturating_add(APPROX_BYTES_PER_TOKEN - 1)
        / APPROX_BYTES_PER_TOKEN
}

fn lines_cost(lines: &[String]) -> usize {
    lines.iter().map(|line| line_cost(line)).sum()
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

        let catalog = SkillCatalog::scan(&workdir, &repo.path().join("user"), 200_000);
        assert_eq!(catalog.skills.len(), 1);
        assert_eq!(catalog.skills[0].description.trim(), "nearest");
        assert!(!catalog.skills[0].allow_implicit_invocation);
        assert!(catalog.validate_explicit("use $same").is_ok());
        assert!(catalog.validate_explicit("use $missing").is_err());
    }

    #[test]
    fn context_budget_shortens_descriptions_before_omitting_skills() {
        let skills = ["alpha", "beta"]
            .into_iter()
            .map(|name| SkillInfo {
                name: name.into(),
                description: "x".repeat(1_000),
                path: PathBuf::from(format!("/tmp/{name}/SKILL.md")),
                allow_implicit_invocation: true,
            })
            .collect::<Vec<_>>();

        let (lines, shortened, omitted) = prompt_lines(&skills, 10_000);

        assert_eq!(lines.len(), 2);
        assert!(shortened);
        assert_eq!(omitted, 0);
        assert!(lines_cost(&lines) <= 200);
    }
}
