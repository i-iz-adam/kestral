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
                "description": "Generate an image from a text prompt and show it directly in the chat. Use this whenever the user asks you to make, draw, render, or visualize an image — do not answer an image request by writing out a prompt for them to paste into some other tool, and do not describe what the image would look like instead of making it. The rendered image is displayed to the user automatically as soon as it's ready, so your reply afterwards should be short: what you made and any choice worth flagging, not a re-statement of the prompt and not a Markdown image embed. Write the prompt as one dense, self-contained description — subject, composition, style, lighting, palette, framing — since the image model sees only this string and none of the conversation. To iterate (\"make it darker\", \"same but from behind\"), call this again with a fully rewritten prompt rather than a delta. Only for images that do not exist yet: if the request concerns an image that is already in the conversation — the user attached one, or you generated one — use edit_image instead, which alters the actual pixels. Re-generating from a description always yields a different subject, however carefully the description is written.",
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
        },
        {
            "type": "function",
            "function": {
                "name": "edit_image",
                "description": "Edit an image that already exists — the user's attachment, or one you generated earlier — instead of rendering a new one from scratch. USE THIS, NOT generate_image, whenever the request is about an existing picture: 'edit this', 'change the X', 'make him look Y', 'remove the background', 'add a hat', 'same but at night', 'fix the hands'. The provider receives the actual pixels and alters them, so the subject, framing and background are preserved; generate_image cannot do that and will produce a different subject even from an identical description. With no source specified it edits the most recent image in the conversation, which is almost always what's wanted. The prompt here describes the CHANGE and the intended result, not the whole scene: 'give the cat a wide-eyed shocked expression, mouth open, ears back — keep everything else identical' rather than a full re-description of the cat and the room. If an edit comes back looking like a different subject, say so and offer to try a tighter prompt or a mask; don't silently pass it off as an edit.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "prompt": {
                            "type": "string",
                            "description": "The change to make, phrased as an instruction and an intended result. Say explicitly what must stay the same ('keep the pose, lighting and background unchanged') — that instruction measurably reduces drift."
                        },
                        "source": {
                            "type": "string",
                            "description": "Which image to edit. Omit for the most recent image in the conversation (the usual case). 'attachment' for the user's most recent attached image specifically, 'generated' for the last image you generated, or a file path (workspace-relative or absolute) for anything else."
                        },
                        "mask": {
                            "type": "string",
                            "description": "Optional path to a PNG mask marking the region to repaint: transparent pixels are edited, opaque pixels are kept. Only pass one you actually have — without a mask the provider edits the whole image guided by the prompt, which is the normal path."
                        },
                        "title": {
                            "type": "string",
                            "description": "Short human label for the result (a few words), used as the filename and the card's caption."
                        },
                        "size": {
                            "type": "string",
                            "description": "Optional output dimensions. Omit to keep the provider's default, which normally matches the source."
                        },
                        "n": {
                            "type": "integer",
                            "description": "How many variations of the edit to produce, 1-4. Defaults to 1."
                        },
                        "model": {
                            "type": "string",
                            "description": "Optional explicit image model id. Editing needs a model that supports it (gpt-image class); omit to use the configured default."
                        },
                        "save_path": {
                            "type": "string",
                            "description": "Optional workspace-relative path to also write the edited image to. Omit for a normal chat reply."
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
    (name == "generate_image" || name == "edit_image")
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
    /// "generate" | "edit" — the card shows the source image being worked
    /// on during an edit, so it has to know which it is from the first
    /// event, before any result exists.
    mode: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    /// The image being edited, so the card can show before/after. Sent on
    /// the edit path only, and only once the source has been resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    source_data_url: Option<String>,
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
    mode: &'a str,
    model: String,
    prompt: String,
    title: String,
    size: String,
    images: Vec<ImageArtifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_data_url: Option<String>,
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
    emit_progress_full(
        app_handle, session_id, call_id, stage, "generate", model, prompt, message, None,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_progress_full(
    app_handle: &tauri::AppHandle,
    session_id: &str,
    call_id: &str,
    stage: &str,
    mode: &str,
    model: Option<String>,
    prompt: Option<String>,
    message: Option<String>,
    source_data_url: Option<String>,
) {
    let _ = app_handle.emit(
        "agent://image-progress",
        ImageProgressEvent {
            session_id,
            call_id,
            stage,
            mode,
            model,
            prompt,
            message,
            source_data_url,
        },
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
    if rel_path.is_absolute() || rel_path.has_root() || rel.contains("..") {
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
    name: &str,
    call_id: &str,
    args: &Value,
) -> Result<String, String> {
    match name {
        "generate_image" => execute_generate(app_handle, session, call_id, args).await,
        "edit_image" => execute_edit(app_handle, session, call_id, args).await,
        other => Err(format!("unknown image tool: {}", other)),
    }
}

async fn execute_generate(
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
        app_handle,
        session_id,
        call_id,
        "resolving",
        None,
        Some(prompt.to_string()),
        None,
    );

    let cfg = config::load_omniroute_config(app_handle)
        .ok_or("No OmniRoute config saved yet — finish setup first")?;

    let model =
        omniroute::resolve_image_model(&cfg, args.get("model").and_then(|v| v.as_str())).await;

    emit_progress(
        app_handle,
        session_id,
        call_id,
        "rendering",
        Some(model.clone()),
        Some(prompt.to_string()),
        None,
    );

    let (generated, effective_model) = match omniroute::generate_image(
        &cfg,
        &model,
        prompt,
        Some(size.as_str()),
        Some(n),
        quality,
        style,
    )
    .await
    {
        Ok(images) => (images, model.clone()),
        Err(e) => {
            if model != "auto" {
                emit_progress(
                    app_handle,
                    session_id,
                    call_id,
                    "rendering",
                    Some("auto".to_string()),
                    Some(prompt.to_string()),
                    None,
                );
                match omniroute::generate_image(
                    &cfg,
                    "auto",
                    prompt,
                    Some(size.as_str()),
                    Some(n),
                    quality,
                    style,
                )
                .await
                {
                    Ok(images) => (images, "auto".to_string()),
                    Err(retry_err) => {
                        emit_progress(
                            app_handle,
                            session_id,
                            call_id,
                            "error",
                            Some(model.clone()),
                            None,
                            Some(retry_err.clone()),
                        );
                        return Err(retry_err);
                    }
                }
            } else {
                emit_progress(
                    app_handle,
                    session_id,
                    call_id,
                    "error",
                    Some(model.clone()),
                    None,
                    Some(e.clone()),
                );
                return Err(e);
            }
        }
    };

    persist_and_report(
        app_handle,
        session,
        call_id,
        &generated,
        FinishParams {
            mode: "generate",
            title: &title,
            prompt,
            size: &size,
            model: &effective_model,
            save_path,
            source_data_url: None,
            source_path: None,
        },
    )
}

/// Everything that happens after a provider hands back pixels: write the
/// files, honor `save_path`, tell the UI, and compose the short result the
/// model sees. Shared by both paths — generation and editing differ only
/// in how they *get* the bytes, and having one place that writes artifacts
/// means the card, the rehydration parser and the result format can't
/// drift apart between them.
struct FinishParams<'a> {
    /// "generate" | "edit"
    mode: &'a str,
    title: &'a str,
    prompt: &'a str,
    size: &'a str,
    model: &'a str,
    save_path: Option<&'a str>,
    source_data_url: Option<String>,
    /// Where the pre-edit image was saved, so a reopened session can
    /// still show the before/after pair. Reported on its own line rather
    /// than as a `- ` bullet, so the result-path parser doesn't mistake
    /// the source for another output.
    source_path: Option<String>,
}

fn persist_and_report(
    app_handle: &tauri::AppHandle,
    session: &Session,
    call_id: &str,
    generated: &[omniroute::GeneratedImage],
    p: FinishParams<'_>,
) -> Result<String, String> {
    let session_id = session.id.as_str();
    let editing = p.mode == "edit";

    emit_progress_full(
        app_handle,
        session_id,
        call_id,
        "saving",
        p.mode,
        Some(p.model.to_string()),
        None,
        None,
        None,
    );

    let dir = artifact_dir(app_handle, session_id)?;
    let slug = slugify(p.title);
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
        if let Some(rel) = p.save_path {
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
            mode: p.mode,
            model: p.model.to_string(),
            prompt: p.prompt.to_string(),
            title: p.title.to_string(),
            size: p.size.to_string(),
            images: artifacts.clone(),
            source_data_url: p.source_data_url.clone(),
        },
    );

    emit_progress_full(
        app_handle,
        session_id,
        call_id,
        "done",
        p.mode,
        Some(p.model.to_string()),
        None,
        None,
        None,
    );

    // Kept short and path-shaped on purpose: this is the text that lands
    // in the conversation forever. The paths are also what the card falls
    // back to when the session is reopened later.
    let plural = artifacts.len() != 1;
    let mut out = if editing {
        format!(
            "Edited the source image with {} — {} result{} now shown in the chat next to the original, so the user can see {} already.\n",
            p.model,
            artifacts.len(),
            if plural { "s" } else { "" },
            if plural { "them" } else { "it" },
        )
    } else {
        format!(
            "Rendered {} image{} with {} at {} and displayed {} in the chat — the user can see {} already.\n",
            artifacts.len(),
            if plural { "s" } else { "" },
            p.model,
            p.size,
            if plural { "them" } else { "it" },
            if plural { "them" } else { "it" },
        )
    };
    for a in &artifacts {
        out.push_str(&format!("- {}\n", a.path));
    }
    if let Some(revised) = artifacts.iter().find_map(|a| a.revised_prompt.clone()) {
        out.push_str(&format!("\nProvider rewrote the prompt as: {}\n", revised));
    }
    for copy in &extra_copies {
        out.push_str(&format!("\nAlso saved into the workspace at {}\n", copy));
    }
    if let Some(src) = &p.source_path {
        out.push_str(&format!("\nsource: {}\n", src));
    }
    if p.save_path.is_some() && plural {
        out.push_str("\nOnly the first variation was copied to save_path; the rest are at the paths above.\n");
    }
    out.push_str(
        "\nDon't embed these paths as Markdown images or re-describe the picture — it's on screen. Say what you made in a line or two, and offer a concrete next tweak if one is worth making.\n",
    );
    if editing {
        out.push_str(
            "If the result changed the subject rather than editing it, tell the user plainly and offer a tighter prompt or a masked edit — don't present a different subject as an edit.\n",
        );
    }
    Ok(out)
}

/// Which image an `edit_image` call should operate on.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SourceSelector {
    /// Most recent image of any kind — attachment or previous render.
    Latest,
    /// Most recent image the *user* attached.
    Attachment,
    /// Most recent image the agent produced.
    Generated,
}

fn parse_selector(raw: Option<&str>) -> Option<SourceSelector> {
    match raw.map(|s| s.trim().to_lowercase()).as_deref() {
        None | Some("") | Some("latest") | Some("last") | Some("current") | Some("this")
        | Some("above") | Some("previous") => Some(SourceSelector::Latest),
        Some("attachment") | Some("attached") | Some("upload") | Some("uploaded")
        | Some("user") | Some("original") => Some(SourceSelector::Attachment),
        Some("generated")
        | Some("generation")
        | Some("last_generated")
        | Some("mine")
        | Some("output") => Some(SourceSelector::Generated),
        // Anything else is treated as a path.
        _ => None,
    }
}

/// Pulls the bytes of an image referenced from a chat message. Attachments
/// arrive as `data:` URLs (that's how the vision path carries them, see
/// omniroute::format_messages_for_llm), but a message can also carry a
/// plain path, so both are handled.
fn bytes_from_message_ref(reference: &str) -> Option<Vec<u8>> {
    let reference = reference.trim();
    if reference.is_empty() {
        return None;
    }
    if reference.starts_with("data:") {
        use base64::Engine as _;
        let payload = reference.split_once(";base64,").map(|(_, tail)| tail)?;
        return base64::engine::general_purpose::STANDARD
            .decode(payload.trim())
            .ok();
    }
    fs::read(reference).ok()
}

/// Walks the conversation backwards for the image to edit.
///
/// This exists because the model cannot pass the image itself: an
/// attachment is a megabyte-plus data URL living in the message history,
/// and a previous render is a file on disk referenced only by a path in a
/// tool result. So "edit this" has to be resolved here, from the session,
/// rather than by the model quoting bytes into a tool argument.
///
/// Newest-first, and it considers both kinds of source, so "make the cat
/// shocked" right after an attachment picks the attachment, while "now
/// make it night" right after a render picks the render — which is what
/// each phrasing means in context.
fn find_source_in_session(
    session: &Session,
    selector: SourceSelector,
) -> Option<(Vec<u8>, String)> {
    for message in session.messages.iter().rev() {
        let is_user = message.role == "user";
        let is_tool = message.role == "tool";

        // An attachment on a user message.
        if is_user && selector != SourceSelector::Generated {
            if let Some(images) = &message.images {
                if let Some(bytes) = images.iter().rev().find_map(|r| bytes_from_message_ref(r)) {
                    return Some((bytes, "attachment".to_string()));
                }
            }
        }

        // A previous render: the tool result holds the artifact paths.
        if is_tool && selector != SourceSelector::Attachment {
            if let Some(content) = &message.content {
                if let Some(path) = artifact_paths_in_result(content).pop() {
                    if let Ok(bytes) = fs::read(&path) {
                        return Some((bytes, path));
                    }
                }
            }
        }

        // An image attached to any other role (an assistant reply that
        // carried one, say) still counts as "latest".
        if !is_user && !is_tool && selector == SourceSelector::Latest {
            if let Some(images) = &message.images {
                if let Some(bytes) = images.iter().rev().find_map(|r| bytes_from_message_ref(r)) {
                    return Some((bytes, "image in conversation".to_string()));
                }
            }
        }
    }
    None
}

/// The `- <path>` lines `persist_and_report` writes, in order. Mirrors the
/// frontend's `parseArtifactPaths` — the two are the same contract read
/// from both ends, which is why the result format is a list of bare paths
/// and not prose.
fn artifact_paths_in_result(result: &str) -> Vec<String> {
    result
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix("- ")?.trim();
            let lower = rest.to_lowercase();
            let looks_like_image = [".png", ".jpg", ".jpeg", ".webp", ".gif"]
                .iter()
                .any(|ext| lower.ends_with(ext));
            if looks_like_image {
                Some(rest.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Reads an explicitly named source: absolute, or relative to the
/// workspace. Same containment rule as writing — a path argument from the
/// model shouldn't be able to read arbitrary files off the machine just
/// because the destination is an image endpoint.
fn read_source_path(workspace: &str, raw: &str) -> Result<Vec<u8>, String> {
    let candidate = Path::new(raw);
    let resolved = if candidate.is_absolute() || candidate.has_root() {
        candidate.to_path_buf()
    } else {
        if raw.contains("..") {
            return Err("source path must stay inside the workspace".to_string());
        }
        if workspace.trim().is_empty() {
            return Err(format!(
                "no workspace set, so the relative path {} can't be resolved — pass an absolute path",
                raw
            ));
        }
        Path::new(workspace).join(candidate)
    };
    fs::read(&resolved)
        .map_err(|e| format!("could not read source image {}: {}", resolved.display(), e))
}

async fn execute_edit(
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
        .map(|s| s.to_string());

    let n = args
        .get("n")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .clamp(1, MAX_IMAGES_PER_CALL);

    let save_path = args
        .get("save_path")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let session_id = session.id.as_str();
    let source_arg = args.get("source").and_then(|v| v.as_str());

    emit_progress_full(
        app_handle,
        session_id,
        call_id,
        "resolving",
        "edit",
        None,
        Some(prompt.to_string()),
        None,
        None,
    );

    // Resolve the source before anything else: an unresolvable source is
    // the one failure worth reporting *before* spending a provider call,
    // and the error tells the model how to fix the call rather than just
    // that it failed.
    let (source_bytes, source_label) = match parse_selector(source_arg) {
        Some(selector) => find_source_in_session(session, selector).ok_or_else(|| {
            let what = match selector {
                SourceSelector::Attachment => "attached image",
                SourceSelector::Generated => "previously generated image",
                SourceSelector::Latest => "image",
            };
            format!(
                "no {} found in this conversation to edit. If the user is referring to a file, pass its path as `source`; if they want something new, use generate_image instead.",
                what
            )
        })?,
        None => {
            let raw = source_arg.unwrap_or_default();
            let bytes = read_source_path(&session.workspace, raw)?;
            (bytes, raw.to_string())
        }
    };

    if source_bytes.is_empty() {
        return Err("the source image resolved to zero bytes".to_string());
    }

    let source = omniroute::SourceImage::new(source_bytes, "source");
    let source_data_url = to_data_url(&source.mime, &source.bytes);

    // Keep a copy of what went in. An attachment otherwise exists only as
    // a data URL inside the message history, so without this the
    // before/after pair vanishes the moment the app restarts — and the
    // original is the one thing you can't re-derive if the edit is the
    // keeper.
    let source_path = {
        let dir = artifact_dir(app_handle, session_id)?;
        let name = format!(
            "{}-{}-source.{}",
            slugify(&title),
            timestamp_ms(),
            omniroute::mime_extension(&source.mime)
        );
        let path = dir.join(name);
        match fs::write(&path, &source.bytes) {
            Ok(()) => Some(path.display().to_string()),
            // Not fatal: the edit itself is what was asked for, and the
            // live card already has the bytes for its before/after.
            Err(_) => None,
        }
    };

    let mask = match args
        .get("mask")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => {
            let bytes = read_source_path(&session.workspace, raw)?;
            Some(omniroute::SourceImage::new(bytes, "mask"))
        }
        None => None,
    };

    let cfg = config::load_omniroute_config(app_handle)
        .ok_or("No OmniRoute config saved yet — finish setup first")?;

    let model =
        omniroute::resolve_image_model(&cfg, args.get("model").and_then(|v| v.as_str())).await;

    emit_progress_full(
        app_handle,
        session_id,
        call_id,
        "rendering",
        "edit",
        Some(model.clone()),
        Some(prompt.to_string()),
        None,
        Some(source_data_url.clone()),
    );

    let sources = [source];
    let edited = match omniroute::edit_image(
        &cfg,
        &model,
        prompt,
        &sources,
        mask.as_ref(),
        size.as_deref(),
        Some(n),
    )
    .await
    {
        Ok(images) => images,
        Err(e) => {
            emit_progress_full(
                app_handle,
                session_id,
                call_id,
                "error",
                "edit",
                Some(model.clone()),
                None,
                Some(e.clone()),
                Some(source_data_url.clone()),
            );
            // Note the source in the error: the most common cause is a
            // model that can't edit, and the model's next move should be
            // to say so rather than quietly generating a lookalike.
            return Err(format!("{}\n(source was: {})", e, source_label));
        }
    };

    persist_and_report(
        app_handle,
        session,
        call_id,
        &edited,
        FinishParams {
            mode: "edit",
            title: &title,
            prompt,
            size: size.as_deref().unwrap_or("source size"),
            model: &model,
            save_path,
            source_data_url: Some(source_data_url),
            source_path,
        },
    )
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
        fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {}", parent.display(), e))?;
    }

    if let Some(src) = source.map(str::trim).filter(|s| !s.is_empty()) {
        if Path::new(src).is_file() {
            fs::copy(src, &dest).map_err(|e| format!("could not save image: {}", e))?;
            return Ok(dest.display().to_string());
        }
    }

    if let Some(url) = data_url.map(str::trim).filter(|s| !s.is_empty()) {
        use base64::Engine as _;
        let payload = url
            .split_once(";base64,")
            .map(|(_, tail)| tail)
            .unwrap_or(url);
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
    fn selector_parsing_falls_through_to_paths() {
        assert_eq!(parse_selector(None), Some(SourceSelector::Latest));
        assert_eq!(parse_selector(Some("  ")), Some(SourceSelector::Latest));
        assert_eq!(
            parse_selector(Some("Attached")),
            Some(SourceSelector::Attachment)
        );
        assert_eq!(
            parse_selector(Some("generated")),
            Some(SourceSelector::Generated)
        );
        // Anything unrecognized is a path, not an error — that's how a
        // file source reaches read_source_path.
        assert_eq!(parse_selector(Some("assets/cat.png")), None);
    }

    #[test]
    fn artifact_paths_round_trip_the_result_format() {
        let result = "Edited the source image with m — 2 results now shown.\n- /tmp/a-1.png\n- /tmp/a-2.png\n\nDon't embed these paths\n";
        assert_eq!(
            artifact_paths_in_result(result),
            vec!["/tmp/a-1.png".to_string(), "/tmp/a-2.png".to_string()]
        );
        // Prose bullets aren't paths.
        assert!(artifact_paths_in_result("- not an image\n").is_empty());
    }

    #[test]
    fn source_paths_are_contained() {
        assert!(read_source_path("/ws", "../secrets.png").is_err());
        assert!(read_source_path("", "rel.png").is_err());
    }

    #[test]
    fn data_url_refs_decode() {
        // "hi" in base64.
        assert_eq!(
            bytes_from_message_ref("data:image/png;base64,aGk="),
            Some(b"hi".to_vec())
        );
        assert_eq!(bytes_from_message_ref("   "), None);
    }

    #[test]
    fn only_workspace_writes_need_approval() {
        assert!(!is_mutating_with_args(
            "generate_image",
            &json!({ "prompt": "x" })
        ));
        assert!(is_mutating_with_args(
            "generate_image",
            &json!({ "prompt": "x", "save_path": "a.png" })
        ));
        assert!(!is_mutating_with_args(
            "read_file",
            &json!({ "save_path": "a.png" })
        ));
        assert!(is_mutating_with_args(
            "edit_image",
            &json!({ "prompt": "x", "save_path": "a.png" })
        ));
        assert!(!is_mutating_with_args(
            "edit_image",
            &json!({ "prompt": "x" })
        ));
    }
}
