use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    /// "builtin", "installed" (fetched via install_from_url or hand-authored
    /// by the user), or "learned" (authored by the agent itself, either
    /// directly via create_skill/edit_skill or promoted from a proposal —
    /// see SkillProposal below).
    pub source: String,
    pub enabled: bool,
    /// Trigger words/phrases parsed from this skill's own frontmatter
    /// (`triggers: foo, bar baz`). Builtins additionally fall back to the
    /// hand-picked table in skill_keywords() when this is empty, so
    /// existing builtins keep working without needing frontmatter added
    /// retroactively. Installed/learned skills rely entirely on this field
    /// plus list_skills/read_skill — there's no hardcoded table for them.
    #[serde(default)]
    pub triggers: Vec<String>,
    /// True when a builtin's compiled-in content has been superseded by an
    /// override file the agent (or user) wrote — see get_content(). Always
    /// false for non-builtin skills. Surfaced so the UI can show "modified
    /// from the built-in version" instead of pretending nothing happened.
    #[serde(default)]
    pub overridden: bool,
}

/// A skill create/update the agent proposed on its own initiative (see
/// reflect.rs) rather than one it applied directly via create_skill/
/// edit_skill. Proposals live in their own directory and are never
/// returned by list()/get_content()/find_relevant() — they have zero
/// effect on any session until a human explicitly accepts one. This is
/// the human-in-the-loop half of the self-improvement loop: the agent may
/// notice a pattern worth keeping, but it doesn't get to rewrite its own
/// future instructions unsupervised.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillProposal {
    /// The proposal's own id — a fresh uuid, used only as this queue
    /// entry's filename/lookup key. Never confused with the id of the
    /// skill it proposes to create or update (see target_id).
    pub id: String,
    /// "create" or "update".
    pub kind: String,
    /// For kind == "update": the existing skill's id (builtin, installed,
    /// or learned) this proposes to change. None for kind == "create" —
    /// accept_proposal derives a fresh id from `name` at accept time,
    /// deliberately late, so a name collision is resolved against
    /// whatever skills exist *then*, not whatever existed when the
    /// proposal was written.
    pub target_id: Option<String>,
    pub name: String,
    pub description: String,
    pub content: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Why the reflection pass thinks this is worth keeping — shown to the
    /// human reviewer alongside the diff, never injected into any agent's
    /// context.
    pub rationale: String,
    /// Full prior content, when kind == "update", so the review UI can
    /// render a real diff instead of just the proposed replacement.
    #[serde(default)]
    pub previous_content: Option<String>,
    pub based_on_session: Option<String>,
    pub created_at: u64,
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
        ("skill-creator", "Skill creator", "In-depth methodology for writing, editing, and improving skills — read this before authoring or updating one.", include_str!("../skills_builtin/skill-creator/SKILL.md")),
        ("agents-md", "AGENTS.md authoring", "How to write, structure, and maintain a project's AGENTS.md — the per-repo instructions file Kestrel (and other coding agents) reads automatically.", include_str!("../skills_builtin/agents-md.md")),
        ("decompile-jar", "Decompiling and rebuilding a JVM jar", "Unpacking a jar (including an obfuscated one), decompiling its classes, and iterating on the result until it builds and runs again.", include_str!("../skills_builtin/decompile-jar.md")),
    ];

    raw_builtins
        .into_iter()
        .map(|(id, default_name, default_desc, raw)| {
            let fm = parse_frontmatter(raw);
            let name = if !fm.name.is_empty() && fm.name != "Untitled skill" {
                fm.name
            } else {
                default_name.to_string()
            };
            let description = if !fm.description.is_empty() {
                fm.description
            } else {
                default_desc.to_string()
            };
            BuiltinSkill {
                id,
                name,
                description,
                content: fm.body,
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
    /// "installed" (fetched/hand-authored) or "learned" (agent-authored).
    /// Absent on files written before this field existed, which
    /// deserializes to "installed" via default_installed_origin — the
    /// only skills that predate this field are ones a human fetched by
    /// URL, never ones the agent wrote itself.
    #[serde(default = "default_installed_origin")]
    origin: String,
    #[serde(default)]
    triggers: Vec<String>,
}

fn default_installed_origin() -> String {
    "installed".to_string()
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
        .path()
        .app_config_dir()
        .expect("could not resolve app config dir")
        .join("skills");
    fs::create_dir_all(&dir).ok();
    dir
}

/// Where builtin overrides live: one markdown file per overridden builtin
/// id, e.g. overrides/git-workflow.md. A file here entirely replaces that
/// builtin's compiled-in content (via get_content) without touching the
/// binary — this is how the agent (or the user) can refine a builtin
/// skill's instructions based on what it learns in actual use, without a
/// rebuild. Deleting the override file reverts to the original builtin.
fn overrides_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = skills_dir(app_handle).join("overrides");
    fs::create_dir_all(&dir).ok();
    dir
}

fn override_path(app_handle: &tauri::AppHandle, id: &str) -> PathBuf {
    overrides_dir(app_handle).join(format!("{}.md", id))
}

/// Where the agent's spontaneous, not-yet-reviewed skill proposals live —
/// see SkillProposal. Entirely separate from skills_dir(): nothing here is
/// ever returned by list(), get_content(), or find_relevant(), so a
/// proposal has zero effect on any running session until a human calls
/// accept_proposal.
/// Where a workspace's own skills live: `<workspace>/.kestrel/skills/`,
/// read with exactly the same folder/`.md`/`.json` formats as the global
/// skills_dir() — the only difference is *where* it's rooted. Living
/// inside the workspace (not the app's config dir) is the point: a
/// project skill travels with the repo through git, so "how we deploy
/// this specific project" can be committed and shared the same way
/// AGENTS.md is, rather than living only on one person's machine.
/// Returns None for an empty/missing workspace path rather than creating
/// a stray `.kestrel` next to the app binary or in the cwd.
fn project_skills_dir(workspace: &str) -> Option<PathBuf> {
    if workspace.trim().is_empty() {
        return None;
    }
    let root = PathBuf::from(workspace);
    if !root.is_dir() {
        return None;
    }
    let dir = root.join(".kestrel").join("skills");
    fs::create_dir_all(&dir).ok();
    Some(dir)
}

/// The conventional per-project instructions file a growing number of
/// coding agents (this one included) look for at a workspace's root —
/// see the built-in "agents-md" skill for what belongs in it. Kept as a
/// tiny helper rather than inlined so read_agents_md/agent.rs's context
/// injection and any future write path share one definition of where it
/// lives.
pub fn agents_md_path(workspace: &str) -> Option<PathBuf> {
    if workspace.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(workspace).join("AGENTS.md"))
}

/// Reads a workspace's AGENTS.md if one exists. Used both by the agent
/// loop (to fold it into system context every turn, unconditionally —
/// this is standing project instruction, not a keyword-matched skill) and
/// by the UI (to show whether one exists yet).
pub fn read_agents_md(workspace: &str) -> Option<String> {
    fs::read_to_string(agents_md_path(workspace)?).ok()
}

fn proposals_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path()
        .app_config_dir()
        .expect("could not resolve app config dir")
        .join("skill_proposals");
    fs::create_dir_all(&dir).ok();
    dir
}

fn state_path(app_handle: &tauri::AppHandle) -> PathBuf {
    app_handle
        .path()
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

/// Scans one skills directory (same folder/`.md`/`.json` formats as the
/// global skills_dir) and appends whatever it finds to `out`, skipping
/// any id already present — shared between the global-library scan and
/// the project-library scan in list() so both read exactly the same
/// on-disk shapes. `default_source` is what an entry gets when its own
/// frontmatter/JSON doesn't specify an `origin` — "installed" for the
/// global dir (existing behavior, unchanged), "project" for a workspace's
/// `.kestrel/skills/`.
fn scan_skills_dir(dir: &PathBuf, state: &SkillState, default_source: &str, out: &mut Vec<Skill>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("overrides") {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if skill_md.is_file() {
                if let Ok(data) = fs::read_to_string(&skill_md) {
                    let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
                    let fm = parse_frontmatter(&data);
                    let name = if !fm.name.is_empty() && fm.name != "Untitled skill" { fm.name } else { dir_name.to_string() };
                    let id = dir_name.to_string();
                    if !out.iter().any(|s| s.id == id) {
                        out.push(Skill {
                            id: id.clone(),
                            name,
                            description: fm.description,
                            source: if fm.origin.is_empty() { default_source.to_string() } else { fm.origin },
                            enabled: !state.disabled.contains(&id),
                            triggers: fm.triggers,
                            overridden: false,
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
                                source: file.origin.clone(),
                                enabled: !state.disabled.contains(&file.id),
                                triggers: file.triggers.clone(),
                                overridden: false,
                            });
                        }
                    }
                }
            } else if file_name.ends_with(".md") {
                let id = file_name.trim_end_matches(".md").to_string();
                if let Ok(data) = fs::read_to_string(&path) {
                    let fm = parse_frontmatter(&data);
                    let name = if !fm.name.is_empty() && fm.name != "Untitled skill" { fm.name } else { id.clone() };
                    if !out.iter().any(|s| s.id == id) {
                        out.push(Skill {
                            id: id.clone(),
                            name,
                            description: fm.description,
                            source: if fm.origin.is_empty() { default_source.to_string() } else { fm.origin },
                            enabled: !state.disabled.contains(&id),
                            triggers: fm.triggers,
                            overridden: false,
                        });
                    }
                }
            }
        }
    }
}

/// `workspace`, when given, additionally surfaces that project's own
/// `.kestrel/skills/` library (source "project") — scanned right after
/// builtins and before the global installed/learned library, so a
/// project skill's id takes precedence over a same-id global one (the
/// dedup in scan_skills_dir skips anything list() already contains). A
/// project skill can't shadow a builtin the same way — that still goes
/// through the override mechanism — but shadowing the general skill
/// library is exactly the "this project's way of doing it" case a
/// project-scoped skill exists for.
pub fn list(app_handle: &tauri::AppHandle, workspace: Option<&str>) -> Vec<Skill> {
    let state = load_state(app_handle);
    let mut out = vec![];

    for b in builtin_skills() {
        // An override supersedes the compiled-in name/description/triggers
        // too, not just the body — so an agent-written override can
        // correct a stale description the same way it corrects guidance.
        let override_fm = fs::read_to_string(override_path(app_handle, b.id))
            .ok()
            .map(|raw| parse_frontmatter(&raw));

        let name = override_fm.as_ref().filter(|fm| !fm.name.is_empty() && fm.name != "Untitled skill")
            .map(|fm| fm.name.clone()).unwrap_or_else(|| b.name.clone());
        let description = override_fm.as_ref().filter(|fm| !fm.description.is_empty())
            .map(|fm| fm.description.clone()).unwrap_or_else(|| b.description.clone());
        let triggers = override_fm.as_ref().filter(|fm| !fm.triggers.is_empty())
            .map(|fm| fm.triggers.clone()).unwrap_or_default();

        out.push(Skill {
            id: b.id.to_string(),
            name,
            description,
            source: "builtin".to_string(),
            enabled: !state.disabled.contains(&b.id.to_string()),
            triggers,
            overridden: override_fm.is_some(),
        });
    }

    if let Some(ws) = workspace {
        if let Some(dir) = project_skills_dir(ws) {
            scan_skills_dir(&dir, &state, "project", &mut out);
        }
    }

    scan_skills_dir(&skills_dir(app_handle), &state, "installed", &mut out);

    out
}

pub fn get_content(app_handle: &tauri::AppHandle, id: &str, workspace: Option<&str>) -> Option<String> {
    // An override, if present, wins over everything else — including a
    // compiled-in builtin. This is the mechanism that lets edit_skill
    // "edit" a builtin without a rebuild: the first edit copies the
    // builtin's current content into overrides/<id>.md, and every get_content
    // call (including the one the running agent's own context is built
    // from) reads through it from then on.
    if let Ok(data) = fs::read_to_string(override_path(app_handle, id)) {
        return Some(parse_frontmatter(&data).body);
    }
    if let Some(b) = builtin_skills().into_iter().find(|b| b.id == id) {
        return Some(b.content);
    }
    // A project skill (from this workspace's .kestrel/skills/) is checked
    // before the global library, same precedence as list() — an id that
    // exists in both resolves to the project's version.
    if let Some(ws) = workspace {
        if let Some(dir) = project_skills_dir(ws) {
            let folder_skill = dir.join(id).join("SKILL.md");
            if folder_skill.is_file() {
                if let Ok(data) = fs::read_to_string(folder_skill) {
                    return Some(parse_frontmatter(&data).body);
                }
            }
            let json_path = dir.join(format!("{}.json", id));
            if json_path.is_file() {
                if let Ok(data) = fs::read_to_string(json_path) {
                    if let Ok(file) = serde_json::from_str::<InstalledSkillFile>(&data) {
                        return Some(file.content);
                    }
                }
            }
            let md_path = dir.join(format!("{}.md", id));
            if md_path.is_file() {
                if let Ok(data) = fs::read_to_string(md_path) {
                    return Some(parse_frontmatter(&data).body);
                }
            }
        }
    }
    // Check directory with SKILL.md (folder-based installed/learned skill)
    let folder_skill = skills_dir(app_handle).join(id).join("SKILL.md");
    if folder_skill.is_file() {
        if let Ok(data) = fs::read_to_string(folder_skill) {
            return Some(parse_frontmatter(&data).body);
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
            return Some(parse_frontmatter(&data).body);
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

pub fn delete(app_handle: &tauri::AppHandle, id: &str, workspace: Option<&str>) -> Result<(), String> {
    // A builtin itself can't be deleted, but an override on top of one can
    // — that's just "revert to the original", not "remove the skill".
    let override_file = override_path(app_handle, id);
    if override_file.is_file() {
        return fs::remove_file(override_file).map_err(|e| e.to_string());
    }
    if builtin_skills().iter().any(|b| b.id == id) {
        return Err("Built-in skills can be disabled but not deleted".into());
    }
    if let Some(dir) = workspace.and_then(project_skills_dir) {
        let folder = dir.join(id);
        if folder.is_dir() {
            return fs::remove_dir_all(folder).map_err(|e| e.to_string());
        }
        let json_path = dir.join(format!("{}.json", id));
        if json_path.is_file() {
            return fs::remove_file(json_path).map_err(|e| e.to_string());
        }
        let md_path = dir.join(format!("{}.md", id));
        if md_path.is_file() {
            return fs::remove_file(md_path).map_err(|e| e.to_string());
        }
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

/// Parsed result of a skill file's YAML-ish frontmatter block, see
/// parse_frontmatter.
pub struct FrontMatter {
    pub name: String,
    pub description: String,
    /// Comma-separated `triggers:` line, split and trimmed. Empty if the
    /// file has none — that's normal for hand-authored skills that rely on
    /// list_skills/read_skill instead of automatic keyword loading.
    pub triggers: Vec<String>,
    /// `origin:` line — "installed" or "learned". Empty (not defaulted
    /// here) when absent so callers can tell "no origin line" apart from
    /// an explicit one and apply their own fallback.
    pub origin: String,
    pub body: String,
}

/// Minimal frontmatter parser for skills:
/// ---
/// name: X
/// description: Y
/// triggers: foo, bar baz, .ext
/// origin: learned
/// ---
/// body...
/// Anything without a recognizable frontmatter block is still installed,
/// just with a generic name so it isn't silently dropped. `triggers` and
/// `origin` are optional even inside a well-formed block — most
/// hand-authored and builtin skills won't have either.
pub fn parse_frontmatter(raw: &str) -> FrontMatter {
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
            let mut triggers = Vec::new();
            let mut origin = String::new();
            for line in fm.lines() {
                let line = line.trim();
                if let Some(v) = line.strip_prefix("name:") {
                    name = unquote(v);
                }
                if let Some(v) = line.strip_prefix("description:") {
                    description = unquote(v);
                }
                if let Some(v) = line.strip_prefix("triggers:") {
                    triggers = v
                        .split(',')
                        .map(|t| unquote(t.trim()))
                        .filter(|t| !t.is_empty())
                        .collect();
                }
                if let Some(v) = line.strip_prefix("origin:") {
                    origin = unquote(v);
                }
            }
            return FrontMatter { name, description, triggers, origin, body };
        }
    }
    FrontMatter {
        name: "Untitled skill".to_string(),
        description: String::new(),
        triggers: Vec::new(),
        origin: String::new(),
        body: raw.to_string(),
    }
}

pub async fn install_from_url(app_handle: &tauri::AppHandle, url: &str) -> Result<Skill, String> {
    let client = reqwest::Client::new();
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Fetch failed: {}", resp.status()));
    }
    let raw = resp.text().await.map_err(|e| e.to_string())?;
    let fm = parse_frontmatter(&raw);
    let name = if fm.name.is_empty() { "Untitled skill".to_string() } else { fm.name };

    let id = format!("installed-{}", uuid::Uuid::new_v4());
    let file = InstalledSkillFile {
        id: id.clone(),
        name: name.clone(),
        description: fm.description.clone(),
        content: fm.body,
        origin: "installed".to_string(),
        triggers: fm.triggers.clone(),
    };
    let path = skills_dir(app_handle).join(format!("{}.json", id));
    let data = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    fs::write(&path, data).map_err(|e| e.to_string())?;

    Ok(Skill {
        id,
        name,
        description: fm.description,
        source: "installed".to_string(),
        enabled: true,
        triggers: fm.triggers,
        overridden: false,
    })
}

/// Turns a proposed skill name into a filesystem/id-safe slug: lowercase,
/// non-alphanumeric runs collapsed to a single '-', trimmed. Falls back to
/// a bare uuid if the name has no alphanumeric characters at all (e.g. a
/// name that's just emoji or punctuation).
fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut last_was_dash = true; // suppress a leading dash
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    let slug = out.trim_end_matches('-').to_string();
    if slug.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        slug
    }
}

/// Picks an id that doesn't collide with any existing skill (builtin,
/// installed, or learned) — appending a short uuid suffix if the plain
/// slug is already taken, rather than silently overwriting an unrelated
/// skill that happens to have a similar name.
fn unique_id(app_handle: &tauri::AppHandle, preferred: &str, workspace: Option<&str>) -> String {
    let existing = list(app_handle, workspace);
    if !existing.iter().any(|s| s.id == preferred) {
        return preferred.to_string();
    }
    format!("{}-{}", preferred, &uuid::Uuid::new_v4().to_string()[..8])
}

fn render_skill_md(name: &str, description: &str, triggers: &[String], origin: &str, body: &str) -> String {
    let mut fm = format!("---\nname: {}\ndescription: {}\n", name, description);
    if !triggers.is_empty() {
        fm.push_str(&format!("triggers: {}\n", triggers.join(", ")));
    }
    fm.push_str(&format!("origin: {}\n---\n\n", origin));
    fm.push_str(body.trim_start());
    if !fm.ends_with('\n') {
        fm.push('\n');
    }
    fm
}

/// Creates a brand-new skill (id derived from `name` if not given) or, for
/// a builtin id, writes/updates the override that supersedes its compiled
/// -in content. This is the direct-application path: no review queue, just
/// like write_file — used when a human explicitly asks for a skill right
/// now, or from the agent's own create_skill/edit_skill tool calls when
/// planning mode is off (or the human approves it, when on — both tools
/// are marked mutating, see agent::is_mutating).
/// `workspace` + `project: true` routes a brand-new skill into that
/// workspace's `.kestrel/skills/` instead of the app's global skills
/// dir — irrelevant for updates to an existing skill (its on-disk
/// location, wherever that is, wins) and irrelevant for a builtin id
/// (always an override, same as before; project scope doesn't apply to
/// overriding compiled-in content).
pub fn write_skill(
    app_handle: &tauri::AppHandle,
    id: Option<&str>,
    name: &str,
    description: &str,
    content: &str,
    triggers: &[String],
    workspace: Option<&str>,
    project: bool,
) -> Result<Skill, String> {
    if name.trim().is_empty() {
        return Err("name cannot be empty".into());
    }
    if content.trim().is_empty() {
        return Err("content cannot be empty".into());
    }

    let is_builtin = id.map(|i| builtin_skills().iter().any(|b| b.id == i)).unwrap_or(false);

    if is_builtin {
        let id = id.unwrap();
        let rendered = render_skill_md(name, description, triggers, "installed", content);
        fs::write(override_path(app_handle, id), rendered).map_err(|e| e.to_string())?;
        return Ok(Skill {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            source: "builtin".to_string(),
            enabled: true,
            triggers: triggers.to_vec(),
            overridden: true,
        });
    }

    let already_exists = id.map(|i| list(app_handle, workspace).iter().any(|s| s.id == i)).unwrap_or(false);
    let resolved_id = match id {
        Some(existing) => existing.to_string(),
        None => unique_id(app_handle, &slugify(name), workspace),
    };

    // Where this write lands: an update to an id that already lives in
    // the project library stays there regardless of the `project` flag
    // (editing shouldn't relocate a skill); a brand-new skill goes to the
    // project dir only when explicitly asked for one and a real workspace
    // is available, otherwise falls back to the global dir the same as
    // before this existed.
    let project_dir = workspace.and_then(project_skills_dir);
    let existing_lives_in_project = already_exists
        && project_dir.as_ref().map(|dir| {
            dir.join(&resolved_id).join("SKILL.md").is_file()
                || dir.join(format!("{}.json", resolved_id)).is_file()
                || dir.join(format!("{}.md", resolved_id)).is_file()
        }).unwrap_or(false);
    let is_project = existing_lives_in_project || (!already_exists && project);
    let target_dir = if is_project {
        project_dir.ok_or_else(|| "no workspace is active — project-scoped skills need an open workspace".to_string())?
    } else {
        skills_dir(app_handle)
    };

    // If this id already exists as a JSON-format installed skill (the
    // format install_from_url uses), update that file in place rather than
    // also creating a parallel folder-based copy with the same id — list()
    // would then have two on-disk entries claiming the same id, and only
    // one of them would actually reflect the edit. New skills, and skills
    // that already exist in folder form, use the folder format.
    let json_path = target_dir.join(format!("{}.json", resolved_id));
    let existing_origin = if already_exists {
        list(app_handle, workspace).into_iter().find(|s| s.id == resolved_id).map(|s| s.source)
    } else {
        None
    };
    let origin = existing_origin.unwrap_or_else(|| if is_project { "project".to_string() } else { "learned".to_string() });

    let md_path = target_dir.join(format!("{}.md", resolved_id));

    if json_path.is_file() {
        let file = InstalledSkillFile {
            id: resolved_id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            content: content.to_string(),
            origin: origin.clone(),
            triggers: triggers.to_vec(),
        };
        let data = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        fs::write(&json_path, data).map_err(|e| e.to_string())?;
    } else if md_path.is_file() {
        let rendered = render_skill_md(name, description, triggers, &origin, content);
        fs::write(&md_path, rendered).map_err(|e| e.to_string())?;
    } else {
        let dir = target_dir.join(&resolved_id);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let rendered = render_skill_md(name, description, triggers, &origin, content);
        fs::write(dir.join("SKILL.md"), rendered).map_err(|e| e.to_string())?;
    }

    Ok(Skill {
        id: resolved_id,
        name: name.to_string(),
        description: description.to_string(),
        source: origin,
        enabled: true,
        triggers: triggers.to_vec(),
        overridden: false,
    })
}

/// Patches an existing skill's content in place by exact search-and-
/// replace — same semantics as tools::execute("edit_file", ...), just
/// scoped to a skill's body instead of an arbitrary workspace file, and
/// aware that a builtin id means "write/update the override" rather than
/// "the file doesn't exist yet". `edits` is a list of
/// (old_string, new_string, replace_all).
pub fn edit_skill(
    app_handle: &tauri::AppHandle,
    id: &str,
    edits: &[(String, String, bool)],
    workspace: Option<&str>,
) -> Result<String, String> {
    if edits.is_empty() {
        return Err("edits array is empty — give at least one edit".into());
    }

    let current = get_content(app_handle, id, workspace)
        .ok_or_else(|| format!("no skill with id {} — use create_skill for a new one", id))?;
    let mut content = current;

    for (i, (old, new, replace_all)) in edits.iter().enumerate() {
        if old.is_empty() {
            return Err(format!("edit {} has an empty old_string — every edit needs something to find", i + 1));
        }
        let count = content.matches(old.as_str()).count();
        if count == 0 {
            return Err(format!(
                "edit {}: old_string not found in skill {} — re-read it with read_skill, the content may not match exactly",
                i + 1, id
            ));
        }
        if count > 1 && !replace_all {
            return Err(format!(
                "edit {}: old_string matches {} places in skill {} — include more surrounding context, or set replace_all: true",
                i + 1, count, id
            ));
        }
        content = if *replace_all { content.replace(old.as_str(), new.as_str()) } else { content.replacen(old.as_str(), new.as_str(), 1) };
    }

    // Preserve the existing name/description/triggers — an edit_skill call
    // changes the body, not the metadata (use write_skill directly, or a
    // future rename tool, for that).
    let existing = list(app_handle, workspace).into_iter().find(|s| s.id == id)
        .ok_or_else(|| format!("no skill with id {}", id))?;

    write_skill(app_handle, Some(id), &existing.name, &existing.description, &content, &existing.triggers, workspace, false)?;
    Ok(format!("applied {} edit{} to skill {}", edits.len(), if edits.len() == 1 { "" } else { "s" }, id))
}

// ---- proposals: the reviewed half of the self-improvement loop ----

pub fn propose(app_handle: &tauri::AppHandle, proposal: SkillProposal) -> Result<String, String> {
    let path = proposals_dir(app_handle).join(format!("{}.json", proposal.id));
    let data = serde_json::to_string_pretty(&proposal).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| e.to_string())?;
    Ok(proposal.id)
}

pub fn list_proposals(app_handle: &tauri::AppHandle) -> Vec<SkillProposal> {
    let mut out = vec![];
    if let Ok(entries) = fs::read_dir(proposals_dir(app_handle)) {
        for entry in entries.flatten() {
            if let Ok(data) = fs::read_to_string(entry.path()) {
                if let Ok(p) = serde_json::from_str::<SkillProposal>(&data) {
                    out.push(p);
                }
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

pub fn get_proposal(app_handle: &tauri::AppHandle, id: &str) -> Option<SkillProposal> {
    let path = proposals_dir(app_handle).join(format!("{}.json", id));
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn reject_proposal(app_handle: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let path = proposals_dir(app_handle).join(format!("{}.json", id));
    fs::remove_file(path).map_err(|e| e.to_string())
}

/// Applies a proposal exactly the way create_skill/edit_skill would (an
/// "update" whose target_id happens to be a builtin becomes an override,
/// same as always), then removes it from the queue. The human reviewer
/// gets one more chance to edit `content`/`name`/`description` client-side
/// before calling this — the frontend just needs to PATCH the proposal
/// file (or the caller can re-propose) before accepting; this function
/// applies whatever the stored proposal currently says.
pub fn accept_proposal(app_handle: &tauri::AppHandle, id: &str) -> Result<Skill, String> {
    let proposal = get_proposal(app_handle, id).ok_or("proposal not found")?;

    // Proposals (the reflection-pass review queue) are always global-scoped
    // for now — a project-scoped proposal flow would need SkillProposal to
    // carry which workspace it came from, which nothing currently sets.
    let skill = write_skill(
        app_handle,
        proposal.target_id.as_deref(),
        &proposal.name,
        &proposal.description,
        &proposal.content,
        &proposal.triggers,
        None,
        false,
    )?;

    reject_proposal(app_handle, id).ok(); // remove from queue now that it's applied
    Ok(skill)
}

pub fn tool_definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "list_skills",
                "description": "List available skills (id, name, one-line description, and source — builtin/installed/learned/project). When a workspace is open this includes that project's own .kestrel/skills/ library alongside the global one; a project skill with the same id as a global one takes precedence for this project. Check this before an unfamiliar task or language — a matching skill may be worth reading first. Also check this before create_skill/propose_skill, to see whether something close enough already exists to edit instead of duplicate.",
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
        },
        {
            "type": "function",
            "function": {
                "name": "create_skill",
                "description": "Create a brand-new skill from scratch: a reusable piece of procedural knowledge (a convention, a checklist, a tool-specific gotcha, a house style) that a future session — yours or another one — would benefit from not having to rediscover. Read the skill-creator skill first if you haven't already this session. Check list_skills first too: if something close already exists, use edit_skill on it instead of creating a near-duplicate. Use this directly (rather than propose_skill) only when the user explicitly asked you to save/remember something as a skill right now; for a pattern you noticed on your own that nobody asked about, prefer propose_skill so a human reviews it first.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Optional. A short kebab-case id, e.g. 'lang-elixir'. Derived from name if omitted." },
                        "name": { "type": "string", "description": "Short display name." },
                        "description": { "type": "string", "description": "One line: what this covers and when it's relevant. This is what shows up in list_skills, so make it something a future skim would actually recognize as matching." },
                        "content": { "type": "string", "description": "The full skill body in markdown — the actual instructions, not a summary of them. Write it as durable, generalized guidance (\"when X, do Y because Z\"), not a transcript of this specific task." },
                        "triggers": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional. Words/phrases that should auto-load this skill's content when they appear in a user message (case-insensitive; multi-word or dotted entries match as substrings, single words match as whole tokens). Keep this short and specific — over-broad triggers (e.g. a single common word) will fire on unrelated conversations. Automatic AI-based matching also runs regardless of triggers, so this is an optimization, not the only way a skill gets found."
                        },
                        "scope": {
                            "type": "string",
                            "enum": ["global", "project"],
                            "description": "Optional, defaults to 'global'. 'project' saves this into the current workspace's .kestrel/skills/ folder instead of the app-wide library — use this for anything specific to this one project (its deploy process, its own conventions) that wouldn't make sense in another codebase. Requires an active workspace; a project skill can also be committed to the repo and shared with anyone else working on it."
                        }
                    },
                    "required": ["name", "description", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "edit_skill",
                "description": "Patch an existing skill's content by exact search-and-replace — same mechanics as edit_file. Works on any skill, including a builtin one (editing a builtin writes an override that supersedes its compiled-in content from then on; the original ships unchanged, so this is always reversible — see skill-creator for when overriding a builtin is and isn't appropriate). Use this to fix a skill that turned out to be wrong or incomplete, or to fold in something new you learned that belongs with an existing skill rather than as a new one.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "The skill's id, from list_skills." },
                        "edits": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "old_string": { "type": "string", "description": "Exact text to find in the skill's current content (from read_skill). Must be non-empty." },
                                    "new_string": { "type": "string", "description": "Text to replace it with. Empty deletes the matched text." },
                                    "replace_all": { "type": "boolean", "description": "Replace every occurrence instead of requiring exactly one match. Defaults to false." }
                                },
                                "required": ["old_string", "new_string"]
                            }
                        }
                    },
                    "required": ["id", "edits"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "propose_skill",
                "description": "Suggest a new skill, or a change to an existing one, for a human to review before it takes effect — nothing this call does is visible to any session (including this one) unless and until it's accepted. This is the right tool for something you noticed on your own that seems worth keeping (a repeatable pattern, a correction to existing guidance) but that nobody explicitly asked you to save: it costs nothing to propose and never risks polluting the skill library with something half-baked, unlike create_skill/edit_skill which apply immediately. Always give a clear, honest rationale — that's the main thing the reviewer sees.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "target_id": { "type": "string", "description": "Omit to propose a brand-new skill. Set to an existing skill's id (from list_skills) to propose replacing its content — the reviewer will see a diff against the current content." },
                        "name": { "type": "string" },
                        "description": { "type": "string" },
                        "content": { "type": "string", "description": "Full proposed skill body in markdown." },
                        "triggers": { "type": "array", "items": { "type": "string" } },
                        "rationale": { "type": "string", "description": "Why this is worth keeping: what made it generalizable, what it would have saved to have known upfront." }
                    },
                    "required": ["name", "description", "content", "rationale"]
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
    workspace: Option<&str>,
) -> Option<Result<String, String>> {
    match name {
        "list_skills" => {
            let summary: Vec<String> = list(app_handle, workspace)
                .into_iter()
                .filter(|s| s.enabled)
                .map(|s| format!("{} [{}] — {}: {}", s.id, s.source, s.name, s.description))
                .collect();
            Some(Ok(summary.join("\n")))
        }
        "read_skill" => {
            let id = args.get("id").and_then(|v| v.as_str());
            Some(match id {
                Some(id) => get_content(app_handle, id, workspace)
                    .ok_or_else(|| format!("no skill with id {}", id)),
                None => Err("missing id".to_string()),
            })
        }
        "create_skill" => {
            let name_arg = args.get("name").and_then(|v| v.as_str());
            let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str());
            let id = args.get("id").and_then(|v| v.as_str());
            let is_project = args.get("scope").and_then(|v| v.as_str()) == Some("project");
            let triggers: Vec<String> = args.get("triggers").and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
                .unwrap_or_default();
            Some(match (name_arg, content) {
                (Some(name), Some(content)) => write_skill(app_handle, id, name, description, content, &triggers, workspace, is_project)
                    .map(|s| format!("created skill '{}' (id: {}, scope: {})", s.name, s.id, s.source)),
                _ => Err("missing name or content".to_string()),
            })
        }
        "edit_skill" => {
            let id = args.get("id").and_then(|v| v.as_str());
            let edits = args.get("edits").and_then(|v| v.as_array());
            Some(match (id, edits) {
                (Some(id), Some(edits)) if !edits.is_empty() => {
                    let parsed: Vec<(String, String, bool)> = edits.iter().map(|e| {
                        (
                            e.get("old_string").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            e.get("new_string").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            e.get("replace_all").and_then(|v| v.as_bool()).unwrap_or(false),
                        )
                    }).collect();
                    edit_skill(app_handle, id, &parsed, workspace)
                }
                (Some(_), _) => Err("edits array is missing or empty".to_string()),
                _ => Err("missing id".to_string()),
            })
        }
        "propose_skill" => {
            let name_arg = args.get("name").and_then(|v| v.as_str());
            let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str());
            let rationale = args.get("rationale").and_then(|v| v.as_str());
            let target_id = args.get("target_id").and_then(|v| v.as_str()).map(String::from);
            let triggers: Vec<String> = args.get("triggers").and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
                .unwrap_or_default();
            Some(match (name_arg, content, rationale) {
                (Some(name), Some(content), Some(rationale)) => {
                    let previous_content = target_id.as_deref().and_then(|id| get_content(app_handle, id, workspace));
                    let proposal = SkillProposal {
                        id: uuid::Uuid::new_v4().to_string(),
                        kind: if target_id.is_some() { "update".to_string() } else { "create".to_string() },
                        target_id,
                        name: name.to_string(),
                        description: description.to_string(),
                        content: content.to_string(),
                        triggers,
                        rationale: rationale.to_string(),
                        previous_content,
                        based_on_session: None,
                        created_at: now_ms(),
                    };
                    propose(app_handle, proposal)
                        .map(|id| format!("proposed (id: {}) — a human will review it; it has no effect until accepted", id))
                }
                _ => Err("missing name, content, or rationale".to_string()),
            })
        }
        _ => None,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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
        "agents-md" => &["agents.md", "agent instructions", "agents file", "repo instructions"],
        "decompile-jar" => &["decompile", "decompiler", "decompiling", "unpack jar", "obfuscated", "obfuscation", "cfr", "vineflower", "fernflower", "bytecode", "jar file", ".jar", "procyon"],
        _ => &[],
    }
}

fn matches_keyword(lower: &str, tokens: &std::collections::HashSet<&str>, kw: &str) -> bool {
    let kw = kw.to_lowercase();
    if kw.contains(' ') || kw.contains('.') {
        lower.contains(&kw)
    } else {
        tokens.contains(kw.as_str())
    }
}

/// Scans `text` (a user message) for skill triggers and returns whichever
/// enabled skills matched, ready to be dropped straight into a turn's
/// context. Two sources of triggers feed this: the hand-picked table in
/// skill_keywords() (builtins only, kept for the ones that shipped before
/// frontmatter triggers existed), and each skill's own `triggers:`
/// frontmatter line — which covers builtins with an override that adds
/// triggers, and any installed or learned skill that declares its own.
/// A skill with neither still works fine; it just relies on the model
/// finding it via list_skills/read_skill instead of automatic loading,
/// same as before this existed.
pub fn find_relevant(app_handle: &tauri::AppHandle, text: &str, workspace: Option<&str>) -> Vec<Skill> {
    let lower = text.to_lowercase();
    let tokens: std::collections::HashSet<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();

    list(app_handle, workspace)
        .into_iter()
        .filter(|s| {
            s.enabled
                && (skill_keywords(&s.id).iter().any(|kw| matches_keyword(&lower, &tokens, kw))
                    || s.triggers.iter().any(|kw| matches_keyword(&lower, &tokens, kw)))
        })
        .collect()
}

/// The judgment-based counterpart to find_relevant(): rather than
/// requiring a skill's author to have predicted the exact words a future
/// user would type, this hands the fast/cheap model a plain catalog
/// (id + description only — never full skill content, to keep this call
/// small) of whatever find_relevant's keyword pass *didn't* already
/// catch, and asks it to judge relevance the way a person skimming
/// list_skills would. Keyword matching stays as the free, instant
/// fast-path (see find_relevant); this is the catch-all for paraphrases,
/// synonyms, and skills whose triggers just don't happen to overlap with
/// how someone phrased their message. Returns an empty vec (rather than
/// erroring the turn) on any request failure or malformed response —
/// this is a nice-to-have enhancement to context-loading, never something
/// a turn should be blocked on.
pub async fn find_relevant_ai(
    app_handle: &tauri::AppHandle,
    cfg: &crate::config::OmniRouteConfig,
    workspace: Option<&str>,
    text: &str,
    already_matched: &std::collections::HashSet<String>,
) -> Vec<Skill> {
    let candidates: Vec<Skill> = list(app_handle, workspace)
        .into_iter()
        .filter(|s| s.enabled && !already_matched.contains(&s.id))
        .collect();
    if candidates.is_empty() {
        return vec![];
    }

    let catalog = candidates
        .iter()
        .map(|s| format!("- {} ({}): {}", s.id, s.source, s.description))
        .collect::<Vec<_>>()
        .join("\n");

    let system = crate::omniroute::ChatMessage {
        role: "system".into(),
        content: Some(format!(
            "You decide which of the following reference skills, if any, would genuinely help \
             with the user's message below. Only include a skill if reading it would actually \
             change how the task should be approached — an empty answer is normal and common, \
             don't include something just because it's loosely topical.\n\nAvailable skills:\n{}\n\n\
             Respond with ONLY a JSON array of relevant skill ids, e.g. [\"git-workflow\"] or []. \
             No prose, no markdown fences, no explanation.",
            catalog
        )),
        ..Default::default()
    };
    let user = crate::omniroute::ChatMessage {
        role: "user".into(),
        content: Some(text.to_string()),
        ..Default::default()
    };

    let res = tokio::time::timeout(
        std::time::Duration::from_millis(2000),
        crate::omniroute::chat_completion(cfg, "auto/fast", &[system, user], None),
    )
    .await;

    let Ok(Ok(resp)) = res else {
        return vec![];
    };
    let Some(raw) = resp.content else { return vec![] };
    let ids = parse_id_array(&raw);
    candidates.into_iter().filter(|s| ids.contains(&s.id)).collect()
}

/// Tolerant parse of the fast model's "JSON array of ids" response —
/// strips a markdown code fence if the model added one anyway despite
/// being told not to, since that's a common enough small model quirk to
/// just handle rather than lose a genuinely useful match over.
fn parse_id_array(raw: &str) -> Vec<String> {
    let mut cleaned = raw.trim();
    if let Some(rest) = cleaned.strip_prefix("```json") {
        cleaned = rest.trim();
    } else if let Some(rest) = cleaned.strip_prefix("```") {
        cleaned = rest.trim();
    }
    cleaned = cleaned.trim_end_matches("```").trim();
    serde_json::from_str::<Vec<String>>(cleaned).unwrap_or_default()
}

/// Tools that mutate the active skill set and so, under planning mode,
/// should pause for the same kind of approval write_file/edit_file/
/// apply_patch already get — see agent::is_mutating. propose_skill is
/// deliberately excluded: it never touches an active skill, only the
/// review queue, so there's nothing for planning mode to gate.
pub fn is_mutating(name: &str) -> bool {
    matches!(name, "create_skill" | "edit_skill")
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
        let fm = parse_frontmatter(raw);
        assert_eq!(fm.name, "frontend-design");
        assert_eq!(fm.description, "Guidance for UI design");
        assert_eq!(fm.body, "# Header\nContent here.");
        assert!(fm.triggers.is_empty());
        assert_eq!(fm.origin, "");
    }

    #[test]
    fn test_parse_frontmatter_quoted() {
        let raw = "---\nname: \"my-skill\"\ndescription: 'Description of my skill'\n---\nBody text";
        let fm = parse_frontmatter(raw);
        assert_eq!(fm.name, "my-skill");
        assert_eq!(fm.description, "Description of my skill");
        assert_eq!(fm.body, "Body text");
    }

    #[test]
    fn test_parse_frontmatter_no_block() {
        let raw = "# Git workflow\nNo frontmatter here.";
        let fm = parse_frontmatter(raw);
        assert_eq!(fm.name, "Untitled skill");
        assert_eq!(fm.description, "");
        assert_eq!(fm.body, raw);
    }

    #[test]
    fn test_parse_frontmatter_triggers_and_origin() {
        let raw = "---\nname: my-skill\ndescription: desc\ntriggers: foo, bar baz, .ext\norigin: learned\n---\nBody";
        let fm = parse_frontmatter(raw);
        assert_eq!(fm.triggers, vec!["foo".to_string(), "bar baz".to_string(), ".ext".to_string()]);
        assert_eq!(fm.origin, "learned");
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("My New Skill!"), "my-new-skill");
        assert_eq!(slugify("  Elixir / Phoenix  "), "elixir-phoenix");
    }
}
