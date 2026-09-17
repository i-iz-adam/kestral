//! Lightweight workspace retrieval for relevant prompt context.
//!
//! This is deliberately lexical rather than embedding based: it has no model
//! or network dependency, starts instantly, and gives the agent useful context
//! before it decides which files to open with its normal tools.  The index is
//! rebuilt per request so edits made during a session are visible immediately.

use crate::sessions::Session;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_INDEX_FILES: usize = 8_000;
const MAX_SNIPPETS: usize = 8;
const MAX_CONTEXT_CHARS: usize = 16_000;
const CHUNK_LINES: usize = 24;

/// File types that are useful as prompt context.  Binary files and generated
/// output are intentionally left out even when they happen to have a name
/// that looks text-like.
const TEXT_EXTENSIONS: &[&str] = &[
    "c", "cc", "cpp", "cs", "css", "d", "dart", "ex", "exs", "fish", "go", "h", "hpp", "html",
    "java", "jl", "js", "json", "jsx", "kt", "kts", "less", "lua", "md", "markdown", "mjs", "mts",
    "php", "pl", "py", "rb", "rs", "rst", "sass", "scala", "scss", "sh", "sql", "swift", "text",
    "toml", "ts", "tsx", "txt", "vue", "xml", "yaml", "yml", "zig",
];

const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "__pycache__",
    ".venv",
    "venv",
    ".cache",
    ".turbo",
    "vendor",
];

#[derive(Debug, Clone)]
struct Document {
    path: String,
    text: String,
    is_session_summary: bool,
}

/// One piece of context selected for a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedSnippet {
    /// Workspace-relative path, or `session:<id>` for a prior session summary.
    pub path: String,
    pub content: String,
    pub score: f32,
    pub source: String,
}

/// Result returned by the retriever command and used internally by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResponse {
    pub snippets: Vec<RetrievedSnippet>,
    pub indexed_files: usize,
}

/// A request-scoped index. Keeping this as a value (rather than a global
/// mutable cache) ensures files changed by a tool call are picked up next turn.
#[derive(Debug, Default)]
pub struct WorkspaceIndex {
    documents: Vec<Document>,
}

impl WorkspaceIndex {
    pub fn build(workspace: &str) -> Self {
        let root = Path::new(workspace);
        if workspace.trim().is_empty() || !root.is_dir() {
            return Self::default();
        }

        let mut documents = Vec::new();
        walk(root, root, &mut documents, MAX_INDEX_FILES);
        Self { documents }
    }

    /// Adds compact summaries from other sessions in the same workspace.
    /// Session JSON is stored in the app data directory, not the project, so
    /// it cannot be found by a workspace walk alone.
    pub fn add_session_summaries(
        &mut self,
        sessions: &[Session],
        workspace: &str,
        current_session_id: &str,
    ) {
        let workspace = canonical_or_raw(Path::new(workspace));
        for session in sessions {
            if session.id == current_session_id
                || canonical_or_raw(Path::new(&session.workspace)) != workspace
            {
                continue;
            }
            let summaries: Vec<&str> = session
                .messages
                .iter()
                .filter(|message| message.role == "system")
                .filter_map(|message| message.content.as_deref())
                .filter(|content| is_summary(content))
                .collect();
            if summaries.is_empty() {
                continue;
            }
            let mut text = format!("Session title: {}\n", session.title);
            for summary in summaries {
                text.push_str(summary);
                text.push_str("\n\n");
            }
            // A stale/huge session must not be able to consume the prompt.
            text = truncate(&text, 8_000);
            self.documents.push(Document {
                path: format!("session:{}", session.id),
                text,
                is_session_summary: true,
            });
        }
    }

    pub fn retrieve(&self, query: &str, max_snippets: usize) -> RetrievalResponse {
        let terms = query_terms(query);
        if terms.is_empty() {
            return RetrievalResponse {
                snippets: Vec::new(),
                indexed_files: self.documents.len(),
            };
        }

        let mut candidates = Vec::new();
        for document in &self.documents {
            let path_lower = document.path.to_lowercase();
            let path_hits = terms
                .iter()
                .filter(|term| path_lower.contains(*term))
                .count();
            let chunks = if document.is_session_summary {
                vec![document.text.clone()]
            } else {
                make_chunks(&document.text)
            };
            for chunk in chunks {
                let lower = chunk.to_lowercase();
                let body_hits = terms.iter().filter(|term| lower.contains(*term)).count();
                if body_hits == 0 && path_hits == 0 {
                    continue;
                }
                // Coverage matters more than raw occurrence count, while a
                // path hit makes README/config names discoverable.
                let score = (body_hits as f32 / terms.len() as f32) * 10.0
                    + path_hits as f32 * 1.5
                    + (lower.matches(terms[0].as_str()).count().min(5) as f32 * 0.1);
                candidates.push(RetrievedSnippet {
                    path: document.path.clone(),
                    content: truncate(&chunk, 3_500),
                    score,
                    source: if document.is_session_summary {
                        "session-summary".into()
                    } else {
                        "workspace-file".into()
                    },
                });
            }
        }

        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        let limit = max_snippets.clamp(1, MAX_SNIPPETS);
        let mut snippets = Vec::new();
        let mut total_chars = 0;
        for candidate in candidates {
            // Avoid showing multiple overlapping chunks from one file and
            // avoid a single large project dominating the retrieved context.
            if snippets
                .iter()
                .any(|snippet: &RetrievedSnippet| snippet.path == candidate.path)
            {
                continue;
            }
            if total_chars + candidate.content.chars().count() > MAX_CONTEXT_CHARS
                && !snippets.is_empty()
            {
                break;
            }
            total_chars += candidate.content.chars().count();
            snippets.push(candidate);
            if snippets.len() >= limit {
                break;
            }
        }
        RetrievalResponse {
            snippets,
            indexed_files: self.documents.len(),
        }
    }
}

/// Indexes project text and historical summaries, then returns lexical RAG
/// snippets suitable for inserting into a model's system context.
pub fn retrieve_workspace_context(
    workspace: &str,
    query: &str,
    sessions: &[Session],
    current_session_id: &str,
) -> RetrievalResponse {
    let mut index = WorkspaceIndex::build(workspace);
    index.add_session_summaries(sessions, workspace, current_session_id);
    index.retrieve(query, 6)
}

fn is_summary(content: &str) -> bool {
    let lower = content.trim_start().to_lowercase();
    lower.starts_with("[conversation summary") || lower.starts_with("conversation summary:")
}

fn canonical_or_raw(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn walk(root: &Path, dir: &Path, documents: &mut Vec<Document>, remaining: usize) {
    if remaining == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if documents.len() >= MAX_INDEX_FILES {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| IGNORED_DIRS.contains(&name))
            {
                continue;
            }
            walk(root, &path, documents, remaining.saturating_sub(1));
        } else if is_text_file(&path) {
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            if metadata.len() > MAX_FILE_BYTES {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            if text.trim().is_empty() {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            documents.push(Document {
                path: relative,
                text,
                is_session_summary: false,
            });
        }
    }
}

fn is_text_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            !name.starts_with('.') || name == ".gitignore" || name == ".editorconfig"
        })
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| TEXT_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
}

fn query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for term in query.split(|c: char| !c.is_alphanumeric() && c != '_') {
        let normalized = term.to_lowercase();
        if normalized.chars().count() >= 2 && !terms.contains(&normalized) {
            terms.push(normalized);
        }
    }
    terms
}

fn make_chunks(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }
    lines
        .chunks(CHUNK_LINES)
        .map(|chunk| chunk.join("\n"))
        .collect()
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let value: String = text.chars().take(max_chars).collect();
    format!("{}\n...[snippet truncated]", value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn retrieves_matching_file_and_ignores_binary_or_build_dirs() {
        let root = std::env::temp_dir().join(format!("kestrel-rag-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(
            root.join("README.md"),
            "The enchanted cache stores session knowledge.",
        )
        .unwrap();
        fs::write(root.join("target/generated.txt"), "enchanted cache").unwrap();
        fs::write(root.join("image.png"), "enchanted cache").unwrap();
        let index = WorkspaceIndex::build(root.to_str().unwrap());
        let result = index.retrieve("session knowledge", 4);
        assert_eq!(result.snippets.len(), 1);
        assert_eq!(result.snippets[0].path, "README.md");
        assert_eq!(result.indexed_files, 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn caps_and_deduplicates_retrieved_context() {
        let root = std::env::temp_dir().join(format!("kestrel-rag-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("notes.md"), "retrieval ".repeat(500)).unwrap();
        let result = WorkspaceIndex::build(root.to_str().unwrap()).retrieve("retrieval", 99);
        assert_eq!(result.snippets.len(), 1);
        assert!(result.snippets[0].content.chars().count() < 3_500 + 30);
        fs::remove_dir_all(root).unwrap();
    }
}
