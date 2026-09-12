use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    /// "builtin" or "installed"
    pub source: String,
    pub enabled: bool,
}

struct BuiltinSkill {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    content: &'static str,
}

/// The skill library shipped with the app. Each is a small, focused
/// markdown file under src-tauri/skills_builtin/ — not loaded into every
/// conversation, just made discoverable via the list_skills/read_skill
/// tools so the agent can pull one in only when it's actually relevant.
fn builtin_skills() -> Vec<BuiltinSkill> {
    vec![
        BuiltinSkill {
            id: "git-workflow",
            name: "Git workflow",
            description: "Branch naming, commit conventions, and what to check before opening a PR.",
            content: include_str!("../skills_builtin/git-workflow.md"),
        },
        BuiltinSkill {
            id: "debugging",
            name: "Systematic debugging",
            description: "A repeatable process for isolating and fixing a bug instead of guessing.",
            content: include_str!("../skills_builtin/debugging.md"),
        },
        BuiltinSkill {
            id: "testing",
            name: "Writing and running tests",
            description: "What's worth testing, the write-fail-fix loop, and how to find the right test command.",
            content: include_str!("../skills_builtin/testing.md"),
        },
        BuiltinSkill {
            id: "code-review",
            name: "Self code review",
            description: "A checklist to run over a diff before presenting it as done.",
            content: include_str!("../skills_builtin/code-review.md"),
        },
        BuiltinSkill {
            id: "refactoring",
            name: "Safe refactoring",
            description: "Restructuring code without changing behavior, in small reversible steps.",
            content: include_str!("../skills_builtin/refactoring.md"),
        },
        BuiltinSkill {
            id: "lang-python",
            name: "Python",
            description: "Tooling, idioms, and common gotchas.",
            content: include_str!("../skills_builtin/lang-python.md"),
        },
        BuiltinSkill {
            id: "lang-typescript",
            name: "JavaScript / TypeScript",
            description: "Tooling, idioms, and common gotchas.",
            content: include_str!("../skills_builtin/lang-typescript.md"),
        },
        BuiltinSkill {
            id: "lang-rust",
            name: "Rust",
            description: "Tooling, idioms, and common gotchas.",
            content: include_str!("../skills_builtin/lang-rust.md"),
        },
        BuiltinSkill {
            id: "lang-go",
            name: "Go",
            description: "Tooling, idioms, and common gotchas.",
            content: include_str!("../skills_builtin/lang-go.md"),
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstalledSkillFile {
    id: String,
    name: String,
    description: String,
    content: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SkillState {
    /// Ids explicitly turned off. Absence from this list means enabled —
    /// this way a newly added builtin skill is on by default for
    /// existing users too, matching how the setup-step registry behaves.
    disabled: Vec<String>,
}

fn skills_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path_resolver()
        .app_config_dir()
        .expect("could not resolve app config dir")
        .join("skills");
    fs::create_dir_all(&dir).ok();
    dir
}

fn state_path(app_handle: &tauri::AppHandle) -> PathBuf {
    app_handle
        .path_resolver()
        .app_config_dir()
        .expect("could not resolve app config dir")
        .join("skill_state.json")
}

fn load_state(app_handle: &tauri::AppHandle) -> SkillState {
    fs::read_to_string(state_path(app_handle))
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
        .unwrap_or_default()
}

fn save_state(app_handle: &tauri::AppHandle, state: &SkillState) {
    if let Ok(data) = serde_json::to_string_pretty(state) {
        let _ = fs::write(state_path(app_handle), data);
    }
}

pub fn list(app_handle: &tauri::AppHandle) -> Vec<Skill> {
    let state = load_state(app_handle);
    let mut out = vec![];

    for b in builtin_skills() {
        out.push(Skill {
            id: b.id.to_string(),
            name: b.name.to_string(),
            description: b.description.to_string(),
            source: "builtin".to_string(),
            enabled: !state.disabled.contains(&b.id.to_string()),
        });
    }

    if let Ok(entries) = fs::read_dir(skills_dir(app_handle)) {
        for entry in entries.flatten() {
            if let Ok(data) = fs::read_to_string(entry.path()) {
                if let Ok(file) = serde_json::from_str::<InstalledSkillFile>(&data) {
                    out.push(Skill {
                        id: file.id.clone(),
                        name: file.name,
                        description: file.description,
                        source: "installed".to_string(),
                        enabled: !state.disabled.contains(&file.id),
                    });
                }
            }
        }
    }

    out
}

pub fn get_content(app_handle: &tauri::AppHandle, id: &str) -> Option<String> {
    if let Some(b) = builtin_skills().into_iter().find(|b| b.id == id) {
        return Some(b.content.to_string());
    }
    let path = skills_dir(app_handle).join(format!("{}.json", id));
    let data = fs::read_to_string(path).ok()?;
    let file: InstalledSkillFile = serde_json::from_str(&data).ok()?;
    Some(file.content)
}

pub fn toggle(app_handle: &tauri::AppHandle, id: &str, enabled: bool) {
    let mut state = load_state(app_handle);
    state.disabled.retain(|x| x != id);
    if !enabled {
        state.disabled.push(id.to_string());
    }
    save_state(app_handle, &state);
}

pub fn delete(app_handle: &tauri::AppHandle, id: &str) -> Result<(), String> {
    if builtin_skills().iter().any(|b| b.id == id) {
        return Err("Built-in skills can be disabled but not deleted".into());
    }
    let path = skills_dir(app_handle).join(format!("{}.json", id));
    fs::remove_file(path).map_err(|e| e.to_string())
}

/// Minimal frontmatter parser for installed skills:
/// ---
/// name: X
/// description: Y
/// ---
/// body...
/// Anything without a recognizable frontmatter block is still installed,
/// just with a generic name so it isn't silently dropped.
fn parse_frontmatter(raw: &str) -> (String, String, String) {
    if let Some(rest) = raw.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            let body = rest[end + 4..].trim_start_matches('\n').to_string();
            let mut name = String::new();
            let mut description = String::new();
            for line in fm.lines() {
                if let Some(v) = line.strip_prefix("name:") {
                    name = v.trim().to_string();
                }
                if let Some(v) = line.strip_prefix("description:") {
                    description = v.trim().to_string();
                }
            }
            return (name, description, body);
        }
    }
    ("Untitled skill".to_string(), String::new(), raw.to_string())
}

pub async fn install_from_url(app_handle: &tauri::AppHandle, url: &str) -> Result<Skill, String> {
    let client = reqwest::Client::new();
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Fetch failed: {}", resp.status()));
    }
    let raw = resp.text().await.map_err(|e| e.to_string())?;
    let (name, description, body) = parse_frontmatter(&raw);
    let name = if name.is_empty() { "Untitled skill".to_string() } else { name };

    let id = format!("installed-{}", uuid::Uuid::new_v4());
    let file = InstalledSkillFile {
        id: id.clone(),
        name: name.clone(),
        description: description.clone(),
        content: body,
    };
    let path = skills_dir(app_handle).join(format!("{}.json", id));
    let data = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    fs::write(&path, data).map_err(|e| e.to_string())?;

    Ok(Skill {
        id,
        name,
        description,
        source: "installed".to_string(),
        enabled: true,
    })
}

pub fn tool_definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "list_skills",
                "description": "List available skills (id, name, one-line description). Check this before an unfamiliar task or language — a matching skill may be worth reading first.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_skill",
                "description": "Read the full instructions for one skill by its id (from list_skills).",
                "parameters": {
                    "type": "object",
                    "properties": { "id": { "type": "string" } },
                    "required": ["id"]
                }
            }
        }
    ])
}

/// Returns Some(result) if `name` is a skills tool, None otherwise so the
/// caller can fall through to other tool handlers.
pub fn maybe_execute(
    app_handle: &tauri::AppHandle,
    name: &str,
    args: &Value,
) -> Option<Result<String, String>> {
    match name {
        "list_skills" => {
            let summary: Vec<String> = list(app_handle)
                .into_iter()
                .filter(|s| s.enabled)
                .map(|s| format!("{} — {}: {}", s.id, s.name, s.description))
                .collect();
            Some(Ok(summary.join("\n")))
        }
        "read_skill" => {
            let id = args.get("id").and_then(|v| v.as_str());
            Some(match id {
                Some(id) => get_content(app_handle, id)
                    .ok_or_else(|| format!("no skill with id {}", id)),
                None => Err("missing id".to_string()),
            })
        }
        _ => None,
    }
}

/// Hand-picked trigger words per builtin skill, checked against the
/// user's message so the right skill can be pulled into context
/// automatically (see find_relevant) instead of relying on the model to
/// remember list_skills/read_skill exist and choose to call them — which,
/// being entirely optional from the model's point of view, it often just
/// doesn't. Bare single words are matched as whole tokens (so "go" the
/// builtin skill doesn't fire on every sentence containing the word "go");
/// anything containing a space or a dot is matched as a substring instead.
fn skill_keywords(id: &str) -> &'static [&'static str] {
    match id {
        "git-workflow" => &["git", "commit", "commits", "branch", "rebase", "merge", "pull request", "pr"],
        "debugging" => &["bug", "debug", "debugging", "crash", "crashing", "traceback", "stack trace", "broken", "failing"],
        "testing" => &["test", "tests", "testing", "pytest", "jest", "unit test", "coverage", "tdd"],
        "code-review" => &["review", "code review", "self-review"],
        "refactoring" => &["refactor", "refactoring", "restructure", "restructuring"],
        "lang-python" => &["python", "pip", "django", "flask", "pytest", ".py"],
        "lang-typescript" => &["typescript", "javascript", "react", "vite", "npm", "node", "tsx", ".ts", ".tsx", ".js", ".jsx"],
        "lang-rust" => &["rust", "cargo", "tokio", ".rs"],
        "lang-go" => &["golang", "goroutine", "go.mod", ".go"],
        _ => &[],
    }
}

/// Scans `text` (a user message) for skill triggers and returns whichever
/// enabled builtin skills matched, ready to be dropped straight into a
/// turn's context. Only covers builtins — installed skills have no
/// hand-written keyword list, so they still rely on the model finding
/// them via list_skills/read_skill, same as before.
pub fn find_relevant(app_handle: &tauri::AppHandle, text: &str) -> Vec<Skill> {
    let lower = text.to_lowercase();
    let tokens: std::collections::HashSet<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();

    let enabled = list(app_handle);

    builtin_skills()
        .into_iter()
        .filter(|b| {
            enabled.iter().any(|s| s.id == b.id && s.enabled)
                && skill_keywords(b.id).iter().any(|kw| {
                    if kw.contains(' ') || kw.contains('.') {
                        lower.contains(kw)
                    } else {
                        tokens.contains(kw)
                    }
                })
        })
        .map(|b| Skill {
            id: b.id.to_string(),
            name: b.name.to_string(),
            description: b.description.to_string(),
            source: "builtin".to_string(),
            enabled: true,
        })
        .collect()
}
