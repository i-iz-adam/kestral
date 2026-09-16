//! Image generation as a first-class agent capability: the
//! `generate_image` tool, the progress events that drive the card's
//! animation while a render is in flight, and the on-disk artifact store
//! the chat renders from afterwards.
//!
//! Why the artifacts live on disk rather than inline in the conversation:
//! a 1024x1024 PNG is on the order of a megabyte of base64, and the
//! session file is rewritten on every turn. Keeping the bytes in a file
//! and the *path* in the tool result means a session with twenty
//! generated images stays a few kilobytes, reloads instantly, and the
//! images survive context compaction (which would otherwise eventually
//! summarize them away). The UI gets the bytes two ways: the completion
//! event carries a data: URL so the live card can paint the moment the
//! render lands, and `load_image_artifact` re-reads from disk when an old
//! session is reopened.

use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{Emitter, Manager};

use crate::config;
use crate::omniroute;
use crate::sessions::Session;

/// Ceiling on images per call. Each one is a separate metered render, and
/// more than a handful in one go is almost always a prompt the model
/// should have iterated on instead.
const MAX_IMAGES_PER_CALL: u64 = 4;

/// The tool schema handed to the model, OpenAI function-calling format —
/// kept here rather than in tools.rs for the same reason github/plan/
/// skills keep theirs in their own modules: the definition and the
/// executor stay next to each other.
pub fn tool_definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "generate_image",
                "description": "Generate an image from a text prompt and show it directly in the chat. Use this whenever the user asks you to make, draw, render, or visualize an image — do not answer an image request by writing out a prompt for them to paste into some other tool, and do not describe what the image would look like instead of making it. The rendered image is displayed to the user automatically as soon as it's ready, so your reply afterwards should be short: what you made and any choice worth flagging, not a re-statement of the prompt and not a Markdown image embed. Write the prompt as one dense, self-contained description — subject, composition, style, lighting, palette, framing — since the image model sees only this string and none of the conversation. To iterate (\"make it darker\", \"same but from behind\"), call this again with a fully rewritten prompt rather than a delta.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "prompt": {
                            "type": "string",
                            "description": "Full description of the image to render. Self-contained — the image model sees nothing else."
                        },
                        "title": {
                            "type": "string",
                            "description": "Short human label for the artifact (a few words), used as the filename and the card's caption. Defaults to a slug of the prompt."
                        },
                        "size": {
                            "type": "string",
                            "description": "Pixel dimensions, e.g. '1024x1024' (square, default), '1792x1024' (landscape), '1024x1792' (portrait). Providers accept different sets; square is the safe choice."
                        },
                        "n": {
                            "type": "integer",
                            "description": "How many variations to render, 1-4. Defaults to 1. Each one is a separate metered render — only ask for more than one when the user actually wants options."
                        },
                        "model": {
                            "type": "string",
                            "description": "Optional explicit image model id (e.g. 'openai/gpt-image-2', 'together/flux'). Omit to use the configured default — do not guess an id you haven't seen listed."
                        },
                        "quality": {
                            "type": "string",
                            "description": "Optional provider-specific quality hint (e.g. 'standard', 'hd')."
                        },
                        "style": {
                            "type": "string",
                            "description": "Optional provider-specific style hint (e.g. 'vivid', 'natural')."
                        },
                        "save_path": {
                            "type": "string",
                            "description": "Optional workspace-relative path to also write the image to (e.g. 'assets/boss-render.png'), for when it's a project asset and not just a chat reply. Omit for a normal chat image — it's already saved and shown either way."
                        }
                    },
                    "required": ["prompt"]
                }
            }
        }
    ])
}

/// `generate_image` only counts as mutating — and so only waits for
/// approval in planning mode — when it's asked to write into the
/// workspace. A plain chat render writes to the app's own artifact
/// directory, and gating every image behind an approval click would make
/// the feature tedious for no safety gain.
pub fn is_mutating_with_args(name: &str, args: &Value) -> bool {
    name == "generate_image"
        && args
            .get("save_path")
            .and_then(|v| v.as_str())
            .map_or(false, |s| !s.trim().is_empty())
}

/// Stages the card animates through. Emitted on `agent://image-progress`
/// keyed by call_id, so the card that owns that call can follow along
/// without any of this having to pass through the global agent store.
#[derive(Debug, Clone, Serialize)]
struct ImageProgressEvent<'a> {
    session_id: &'a str,
    call_id: &'a str,
    /// "resolving" | "dispatched" | "rendering" | "saving" | "done" | "error"
    stage: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// One finished image, as the UI needs it. `data_url` is what the live
/// card paints immediately; `path` is what survives a restart.
#[derive(Debug, Clone, Serialize)]
pub struct ImageArtifact {
    pub path: String,
    pub name: String,
    pub mime: String,
    pub data_url: String,
    pub bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ImageReadyEvent<'a> {
    session_id: &'a str,
    call_id: &'a str,
    model: String,
    prompt: String,
    title: String,
    size: String,
    images: Vec<ImageArtifact>,
}

fn emit_progress(
    app_handle: &tauri::AppHandle,
    session_id: &str,
    call_id: &str,
    stage: &str,
    model: Option<String>,
    prompt: Option<String>,
    message: Option<String>,
) {
    let _ = app_handle.emit(
        "agent://image-progress",
        ImageProgressEvent { session_id, call_id, stage, model, prompt, message },
    );
}

pub fn to_data_url(mime: &str, bytes: &[u8]) -> String {
    use base64::Engine as _;
    format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// Where generated images live: the app's own local data directory, one
/// folder per session. Outside the workspace on purpose — a chat image
/// isn't a project file, and dropping megabytes of PNG into someone's git
/// repo uninvited is exactly the kind of surprise a tool shouldn't spring.
/// `save_path` is the opt-in for when it *is* a project asset.
fn artifact_dir(app_handle: &tauri::AppHandle, session_id: &str) -> Result<PathBuf, String> {
    let base = app_handle
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("could not resolve app data dir: {}", e))?;
    let dir = base.join("generated-images").join(session_id);
    fs::create_dir_all(&dir).map_err(|e| format!("could not create image directory: {}", e))?;
    Ok(dir)
}

/// Filesystem-safe slug from a title or prompt, bounded so a 300-word
/// prompt doesn't produce a path the OS rejects.
fn slugify(text: &str) -> String {
    let slug: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let collapsed = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        "image".to_string()
    } else {
        collapsed.chars().take(60).collect()
    }
}

fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Resolves a workspace-relative `save_path`, refusing anything that
/// climbs out of the workspace — same containment rule the file tools
/// apply, for the same reason.
fn resolve_save_path(workspace: &str, rel: &str) -> Result<PathBuf, String> {
    if workspace.trim().is_empty() {
        return Err("this session has no workspace, so save_path can't be resolved".to_string());
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel.contains("..") {
        return Err("save_path must be a relative path inside the workspace".to_string());
    }
    Ok(Path::new(workspace).join(rel_path))
}

/// Renders one `generate_image` call end to end: resolve the model, call
/// OmniRoute, write each image to the artifact directory, tell the UI,
/// and hand the model back a short text result.
///
/// The result text is deliberately terse and path-only. It's what goes
/// into the conversation history, so it has to stay small — and it tells
/// the model the user can already see the image, because the most common
/// failure mode otherwise is the model dutifully re-describing a picture
/// that's sitting right there on screen.
pub async fn execute(
    app_handle: &tauri::AppHandle,
    session: &Session,
    call_id: &str,
    args: &Value,
) -> Result<String, String> {
    let prompt = args
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("missing prompt parameter")?;

    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(prompt)
        .to_string();

    let size = args
        .get("size")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("1024x1024")
        .to_string();

    let n = args
        .get("n")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .clamp(1, MAX_IMAGES_PER_CALL);

    let quality = args.get("quality").and_then(|v| v.as_str());
    let style = args.get("style").and_then(|v| v.as_str());
    let save_path = args
        .get("save_path")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let session_id = session.id.as_str();

    emit_progress(
        app_handle, session_id, call_id, "resolving",
        None, Some(prompt.to_string()), None,
    );

    let cfg = config::load_omniroute_config(app_handle)
        .ok_or("No OmniRoute config saved yet — finish setup first")?;

    let model = omniroute::resolve_image_model(&cfg, args.get("model").and_then(|v| v.as_str())).await;

    emit_progress(
        app_handle, session_id, call_id, "rendering",
        Some(model.clone()), Some(prompt.to_string()), None,
    );

    let generated = match omniroute::generate_image(
        &cfg, &model, prompt, Some(size.as_str()), Some(n), quality, style,
    )
    .await
    {
        Ok(images) => images,
        Err(e) => {
            emit_progress(
                app_handle, session_id, call_id, "error",
                Some(model.clone()), None, Some(e.clone()),
            );
            return Err(e);
        }
    };

    emit_progress(
        app_handle, session_id, call_id, "saving",
        Some(model.clone()), None, None,
    );

    let dir = artifact_dir(app_handle, session_id)?;
    let slug = slugify(&title);
    let stamp = timestamp_ms();

    let mut artifacts: Vec<ImageArtifact> = Vec::new();
    let mut extra_copies: Vec<String> = Vec::new();

    for (idx, image) in generated.iter().enumerate() {
        let ext = omniroute::mime_extension(&image.mime);
        let suffix = if generated.len() > 1 {
            format!("-{}", idx + 1)
        } else {
            String::new()
        };
        let name = format!("{}-{}{}.{}", slug, stamp, suffix, ext);
        let path = dir.join(&name);
        fs::write(&path, &image.bytes)
            .map_err(|e| format!("could not write image to {}: {}", path.display(), e))?;

        // Only the first image honors save_path — asking for four
        // variations and one filename is ambiguous, so the rest stay in
        // the artifact directory and the model is told as much.
        if let Some(rel) = save_path {
            if idx == 0 {
                let target = resolve_save_path(&session.workspace, rel)?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("could not create {}: {}", parent.display(), e))?;
                }
                fs::write(&target, &image.bytes)
                    .map_err(|e| format!("could not write image to {}: {}", target.display(), e))?;
                extra_copies.push(target.display().to_string());
            }
        }

        artifacts.push(ImageArtifact {
            path: path.display().to_string(),
            name,
            mime: image.mime.clone(),
            data_url: to_data_url(&image.mime, &image.bytes),
            bytes: image.bytes.len(),
            revised_prompt: image.revised_prompt.clone(),
        });
    }

    let _ = app_handle.emit(
        "agent://image-ready",
        ImageReadyEvent {
            session_id,
            call_id,
            model: model.clone(),
            prompt: prompt.to_string(),
            title: title.clone(),
            size: size.clone(),
            images: artifacts.clone(),
        },
    );

    emit_progress(
        app_handle, session_id, call_id, "done",
        Some(model.clone()), None, None,
    );

    // Kept short and path-shaped on purpose: this is the text that lands
    // in the conversation forever. The paths are also what the card falls
    // back to when the session is reopened later.
    let mut out = format!(
        "Rendered {} image{} with {} at {} and displayed {} in the chat — the user can see {} already.\n",
        artifacts.len(),
        if artifacts.len() == 1 { "" } else { "s" },
        model,
        size,
        if artifacts.len() == 1 { "it" } else { "them" },
        if artifacts.len() == 1 { "it" } else { "them" },
    );
    for a in &artifacts {
        out.push_str(&format!("- {}\n", a.path));
    }
    if let Some(revised) = artifacts.iter().find_map(|a| a.revised_prompt.clone()) {
        out.push_str(&format!("\nProvider rewrote the prompt as: {}\n", revised));
    }
    for copy in &extra_copies {
        out.push_str(&format!("\nAlso saved into the workspace at {}\n", copy));
    }
    if save_path.is_some() && artifacts.len() > 1 {
        out.push_str("\nOnly the first variation was copied to save_path; the rest are at the paths above.\n");
    }
    out.push_str(
        "\nDon't embed these paths as Markdown images or re-describe the picture — it's on screen. Say what you made in a line or two, and offer a concrete next tweak if one is worth making.\n",
    );
    Ok(out)
}

/// Reads a previously generated image back off disk as a data: URL. This
/// is how a reopened session repaints its images: the session file only
/// kept paths, and going through a command (rather than the asset
/// protocol) means no scope configuration and no difference in behavior
/// between Windows paths and POSIX ones.
pub fn load_artifact(path: &str) -> Result<ImageArtifact, String> {
    let p = Path::new(path);
    let bytes = fs::read(p).map_err(|e| format!("could not read {}: {}", path, e))?;
    let mime = match p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "image/png",
    };
    Ok(ImageArtifact {
        path: path.to_string(),
        name: p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "image.png".to_string()),
        mime: mime.to_string(),
        data_url: to_data_url(mime, &bytes),
        bytes: bytes.len(),
        revised_prompt: None,
    })
}

/// Writes an artifact to wherever the person picked in the save dialog.
/// A byte copy rather than a re-encode, so "Save" hands back exactly what
/// the provider rendered. `data_url` is the fallback for an image that
/// only ever existed in the conversation (an attachment, an inline
/// result) and so has no file of its own to copy from.
pub fn copy_artifact_to(
    source: Option<&str>,
    data_url: Option<&str>,
    destination: &str,
) -> Result<String, String> {
    let dest = PathBuf::from(destination);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {}", parent.display(), e))?;
    }

    if let Some(src) = source.map(str::trim).filter(|s| !s.is_empty()) {
        if Path::new(src).is_file() {
            fs::copy(src, &dest).map_err(|e| format!("could not save image: {}", e))?;
            return Ok(dest.display().to_string());
        }
    }

    if let Some(url) = data_url.map(str::trim).filter(|s| !s.is_empty()) {
        use base64::Engine as _;
        let payload = url.split_once(";base64,").map(|(_, tail)| tail).unwrap_or(url);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload.trim())
            .map_err(|e| format!("could not decode image data: {}", e))?;
        fs::write(&dest, bytes).map_err(|e| format!("could not save image: {}", e))?;
        return Ok(dest.display().to_string());
    }

    Err("nothing to save — the image has neither a file on disk nor inline data".to_string())
}

/// Raw bytes for clipboard use, base64-encoded for the IPC hop. The
/// frontend turns this back into a Blob and hands it to the clipboard —
/// keeping the encode/decode here means the webview never has to touch
/// the filesystem.
pub fn artifact_base64(path: &str) -> Result<String, String> {
    use base64::Engine as _;
    let bytes = fs::read(path).map_err(|e| format!("could not read {}: {}", path, e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_bounded_and_safe() {
        assert_eq!(slugify("Chambers of Guru!"), "chambers-of-guru");
        assert_eq!(slugify("   "), "image");
        assert_eq!(slugify("a/b\\c:d"), "a-b-c-d");
        // Long prompts get capped rather than producing an unusable path.
        let long = slugify(&"word ".repeat(40));
        assert!(long.len() <= 60);
        assert_eq!(long.split('-').count(), 8);
    }

    #[test]
    fn save_path_must_stay_in_workspace() {
        assert!(resolve_save_path("/ws", "../escape.png").is_err());
        assert!(resolve_save_path("/ws", "/abs/escape.png").is_err());
        assert!(resolve_save_path("", "ok.png").is_err());
        assert!(resolve_save_path("/ws", "assets/ok.png").is_ok());
    }

    #[test]
    fn only_workspace_writes_need_approval() {
        assert!(!is_mutating_with_args("generate_image", &json!({ "prompt": "x" })));
        assert!(is_mutating_with_args(
            "generate_image",
            &json!({ "prompt": "x", "save_path": "a.png" })
        ));
        assert!(!is_mutating_with_args("read_file", &json!({ "save_path": "a.png" })));
    }
}
