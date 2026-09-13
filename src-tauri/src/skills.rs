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
    name: String,
    description: String,
    content: String,
}

/// The skill library shipped with the app. Each is a small, focused
/// markdown file under src-tauri/skills_builtin/ — not loaded into every
/// conversation, just made discoverable via the list_skills/read_skill
/// tools so the agent can pull one in only when it's actually relevant.
fn builtin_skills() -> Vec<BuiltinSkill> {
    let raw_builtins = vec![
        ("git-workflow", "Git workflow", "Branch naming, commit conventions, and what to check before opening a PR.", include_str!("../skills_builtin/git-workflow.md")),
        ("debugging", "Systematic debugging", "A repeatable process for isolating and fixing a bug instead of guessing.", include_str!("../skills_builtin/debugging.md")),
        ("testing", "Writing and running tests", "What's worth testing, the write-fail-fix loop, and how to find the right test command.", include_str!("../skills_builtin/testing.md")),
        ("code-review", "Self code review", "A checklist to run over a diff before presenting it as done.", include_str!("../skills_builtin/code-review.md")),
        ("refactoring", "Safe refactoring", "Restructuring code without changing behavior, in small reversible steps.", include_str!("../skills_builtin/refactoring.md")),
        ("lang-python", "Python", "Tooling, idioms, and common gotchas.", include_str!("../skills_builtin/lang-python.md")),
        ("lang-typescript", "JavaScript / TypeScript", "Tooling, idioms, and common gotchas.", include_str!("../skills_builtin/lang-typescript.md")),
        ("lang-rust", "Rust", "Tooling, idioms, and common gotchas.", include_str!("../skills_builtin/lang-rust.md")),
        ("lang-go", "Go", "Tooling, idioms, and common gotchas.", include_str!("../skills_builtin/lang-go.md")),
        ("docx", "docx", "Word document creation, editing, and analysis.", include_str!("../skills_builtin/docx/SKILL.md")),
        ("file-reading", "file-reading", "Reading and inspecting uploaded files.", include_str!("../skills_builtin/file-reading/SKILL.md")),
        ("frontend-design", "frontend-design", "Distinctive, intentional visual design guidance.", include_str!("../skills_builtin/frontend-design/SKILL.md")),
        ("pdf", "pdf", "PDF processing guide, creation, merging, and forms.", include_str!("../skills_builtin/pdf/SKILL.md")),
        ("pdf-reading", "pdf-reading", "Reading and inspecting PDF files.", include_str!("../skills_builtin/pdf-reading/SKILL.md")),
        ("pptx", "pptx", "PowerPoint creation, editing, and analysis.", include_str!("../skills_builtin/pptx/SKILL.md")),
        ("xlsx", "xlsx", "Spreadsheet creation, editing, and analysis.", include_str!("../skills_builtin/xlsx/SKILL.md")),
    ];

    raw_builtins
        .into_iter()
        .map(|(id, default_name, default_desc, raw)| {
            let (fm_name, fm_desc, body) = parse_frontmatter(raw);
            let name = if !fm_name.is_empty() && fm_name != "Untitled skill" {
                fm_name
            } else {
                default_name.to_string()
            };
            let description = if !fm_desc.is_empty() {
                fm_desc
            } else {
                default_desc.to_string()
            };
            BuiltinSkill {
                id,
                name,
                description,
                content: body,
            }
        })
        .collect()
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
            name: b.name.clone(),
            description: b.description.clone(),
            source: "builtin".to_string(),
            enabled: !state.disabled.contains(&b.id.to_string()),
        });
    }

    if let Ok(entries) = fs::read_dir(skills_dir(app_handle)) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.is_file() {
                    if let Ok(data) = fs::read_to_string(&skill_md) {
                        let dir_name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown");
                        let (name, description, _) = parse_frontmatter(&data);
                        let name = if !name.is_empty() && name != "Untitled skill" {
                            name
                        } else {
                            dir_name.to_string()
                        };
                        let id = dir_name.to_string();
                        if !out.iter().any(|s| s.id == id) {
                            out.push(Skill {
                                id: id.clone(),
                                name,
                                description,
                                source: "installed".to_string(),
                                enabled: !state.disabled.contains(&id),
                            });
                        }
                    }
                }
            } else if path.is_file() {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if file_name.ends_with(".json") {
                    if let Ok(data) = fs::read_to_string(&path) {
                        if let Ok(file) = serde_json::from_str::<InstalledSkillFile>(&data) {
                            if !out.iter().any(|s| s.id == file.id) {
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
                } else if file_name.ends_with(".md") {
                    let id = file_name.trim_end_matches(".md").to_string();
                    if let Ok(data) = fs::read_to_string(&path) {
                        let (name, description, _) = parse_frontmatter(&data);
                        let name = if !name.is_empty() && name != "Untitled skill" {
                            name
                        } else {
                            id.clone()
                        };
                        if !out.iter().any(|s| s.id == id) {
                            out.push(Skill {
                                id: id.clone(),
                                name,
                                description,
                                source: "installed".to_string(),
                                enabled: !state.disabled.contains(&id),
                            });
                        }
                    }
                }
            }
        }
    }

    out
}

pub fn get_content(app_handle: &tauri::AppHandle, id: &str) -> Option<String> {
    if let Some(b) = builtin_skills().into_iter().find(|b| b.id == id) {
        return Some(b.content);
    }
    // Check directory with SKILL.md (folder-based installed skill)
    let folder_skill = skills_dir(app_handle).join(id).join("SKILL.md");
    if folder_skill.is_file() {
        if let Ok(data) = fs::read_to_string(folder_skill) {
            let (_, _, body) = parse_frontmatter(&data);
            return Some(body);
        }
    }
    // Check JSON file
    let json_path = skills_dir(app_handle).join(format!("{}.json", id));
    if json_path.is_file() {
        if let Ok(data) = fs::read_to_string(json_path) {
            if let Ok(file) = serde_json::from_str::<InstalledSkillFile>(&data) {
                return Some(file.content);
            }
        }
    }
    // Check markdown file
    let md_path = skills_dir(app_handle).join(format!("{}.md", id));
    if md_path.is_file() {
        if let Ok(data) = fs::read_to_string(md_path) {
            let (_, _, body) = parse_frontmatter(&data);
            return Some(body);
        }
    }
    None
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
    let dir = skills_dir(app_handle).join(id);
    if dir.is_dir() {
        return fs::remove_dir_all(dir).map_err(|e| e.to_string());
    }
    let json_path = skills_dir(app_handle).join(format!("{}.json", id));
    if json_path.is_file() {
        return fs::remove_file(json_path).map_err(|e| e.to_string());
    }
    let md_path = skills_dir(app_handle).join(format!("{}.md", id));
    if md_path.is_file() {
        return fs::remove_file(md_path).map_err(|e| e.to_string());
    }
    Err(format!("Skill {} not found", id))
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        if s.len() >= 2 {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// Minimal frontmatter parser for skills:
/// ---
/// name: X
/// description: Y
/// ---
/// body...
/// Anything without a recognizable frontmatter block is still installed,
/// just with a generic name so it isn't silently dropped.
pub fn parse_frontmatter(raw: &str) -> (String, String, String) {
    let trimmed = raw.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---") {
        let rest = rest.trim_start_matches('\r').strip_prefix('\n').unwrap_or(rest);
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            let body = rest[end + 4..]
                .trim_start_matches(|c| c == '\r' || c == '\n')
                .to_string();
            let mut name = String::new();
            let mut description = String::new();
            for line in fm.lines() {
                let line = line.trim();
                if let Some(v) = line.strip_prefix("name:") {
                    name = unquote(v);
                }
                if let Some(v) = line.strip_prefix("description:") {
                    description = unquote(v);
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
        "frontend-design" => &["frontend-design", "frontend design", "ui design", "visual design", "design lead", "aesthetic", "typography", "palette", "layout concept"],
        "docx" => &["docx", "dotx", "word doc", "word document", ".docx", ".dotx"],
        "file-reading" => &["file-reading", "file reading", "read file", "uploaded file", "uploaded_files", "extract-text", "/mnt/user-data/uploads/"],
        "pdf" => &["pdf", ".pdf", "pypdf", "pdfplumber", "reportlab", "qpdf"],
        "pdf-reading" => &["pdf-reading", "pdf reading", "read pdf", "scanned pdf", "pdftotext", "pdfinfo", "pdffonts"],
        "pptx" => &["pptx", "potx", "powerpoint", "presentation", "slide deck", ".pptx", ".potx"],
        "xlsx" => &["xlsx", "xlsm", "xls", "excel", "spreadsheet", "openpyxl", ".xlsx", ".xlsm"],
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
                && skill_keywords(&b.id).iter().any(|kw| {
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

/// Returns set of skill IDs that are already loaded in the session context.
pub fn get_loaded_skill_ids(session: &crate::sessions::Session) -> std::collections::HashSet<String> {
    let mut loaded = std::collections::HashSet::new();
    for msg in &session.messages {
        if msg.role == "skill-loaded" {
            if let Some(content) = &msg.content {
                if let Ok(val) = serde_json::from_str::<Value>(content) {
                    if let Some(id) = val.get("args").and_then(|a| a.get("skill_id")).and_then(|v| v.as_str()) {
                        loaded.insert(id.to_string());
                    }
                }
            }
        }
        if msg.role == "assistant" {
            if let Some(calls) = &msg.tool_calls {
                for call in calls {
                    if call.function.name == "read_skill" {
                        if let Ok(args) = serde_json::from_str::<Value>(&call.function.arguments) {
                            if let Some(id) = args.get("id").and_then(|v| v.as_str()) {
                                loaded.insert(id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    loaded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_standard() {
        let raw = "---\nname: frontend-design\ndescription: Guidance for UI design\n---\n\n# Header\nContent here.";
        let (name, desc, body) = parse_frontmatter(raw);
        assert_eq!(name, "frontend-design");
        assert_eq!(desc, "Guidance for UI design");
        assert_eq!(body, "# Header\nContent here.");
    }

    #[test]
    fn test_parse_frontmatter_quoted() {
        let raw = "---\nname: \"my-skill\"\ndescription: 'Description of my skill'\n---\nBody text";
        let (name, desc, body) = parse_frontmatter(raw);
        assert_eq!(name, "my-skill");
        assert_eq!(desc, "Description of my skill");
        assert_eq!(body, "Body text");
    }

    #[test]
    fn test_parse_frontmatter_no_block() {
        let raw = "# Git workflow\nNo frontmatter here.";
        let (name, desc, body) = parse_frontmatter(raw);
        assert_eq!(name, "Untitled skill");
        assert_eq!(desc, "");
        assert_eq!(body, raw);
    }
}
