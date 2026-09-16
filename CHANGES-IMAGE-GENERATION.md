# Changes: image generation

Frontend verified with `tsc --noEmit` and `vite build` (both clean). The
Rust side was hand-reviewed but **not** compiled in the environment this
was written in — run `cargo check` first; `Cargo.lock` needs regenerating
because of the one new dependency (`base64 = "0.22"`).

## 1. Image client — `src-tauri/src/omniroute.rs`
- `generate_image(cfg, model, prompt, size, n, quality, style)` posts to OmniRoute's `POST /v1/images/generations` and returns `Vec<GeneratedImage>` (raw bytes + sniffed MIME + optional `revised_prompt`). Asks for `response_format: "b64_json"` but handles both inline base64 and hosted-URL responses, plus the several key names providers use for each (`b64_json`/`b64`/`image_base64`/`base64`, `url`/`image_url`, and bare strings), since OpenAI, xAI, FLUX-via-Together/Nebius, NanoBanana and local SD WebUI/ComfyUI don't agree on the shape.
- MIME comes from magic bytes (`sniff_image_mime`), not from a `Content-Type` header or a guess — that's what the saved extension and the data: URL both derive from.
- Deliberately **not** covered by the retry/backoff in §4 of the previous pass: an image call is slow and metered per image, so a silent retry risks paying twice for a request that may have succeeded upstream. Failures come back with the provider's own message plus a status-specific hint (404 = endpoint missing, 400/422 = wrong model or unsupported size, 429 = out of quota).
- `list_image_models` (via `GET /v1/images/generations`, falling back to filtering `/v1/models` by modality) and `resolve_image_model` (explicit arg → saved default → first discovered → `openai/gpt-image-2`), so generation works on a fresh install without a settings trip.
- 600s request timeout — reqwest's default is *no* timeout, and a cold local ComfyUI render can legitimately take minutes.

## 2. The tool — `src-tauri/src/images.rs` (new)
- `generate_image` tool schema + executor. Args: `prompt`, `title`, `size`, `n` (1–4), `model`, `quality`, `style`, `save_path`.
- Artifacts are written to `app_local_data_dir/generated-images/<session_id>/`, **not** the workspace: a chat image isn't a project file, and dropping megabytes of PNG into someone's git repo uninvited is exactly the surprise a tool shouldn't spring. `save_path` (workspace-relative, containment-checked, `..` and absolute paths refused) is the opt-in for when it genuinely is an asset.
- The tool result handed back to the model is short and path-shaped, never base64. This is what lands in `session.messages` forever — a session with twenty images stays a few kilobytes, reloads instantly, and the images survive context compaction (§1 of the previous pass) by construction. It also tells the model the user can already see the image, because the reliable failure mode otherwise is the model dutifully re-describing a picture that's on screen.
- `is_mutating_with_args` returns true **only** when `save_path` is set, so planning mode doesn't make you approve every picture — mirrors how `run_python` is only mutating with `workspace_access: true`.
- Events: `agent://image-progress` (`resolving` → `rendering` → `saving` → `done`/`error`, keyed by `call_id`) and `agent://image-ready` (carries data: URLs so the live card paints immediately).
- `load_artifact` / `copy_artifact_to` / `artifact_base64` back the card's rehydration, Save and Copy. `copy_artifact_to` takes a path *or* a data: URL, so the same card works for an image that only ever existed inline.
- Unit tests cover slug bounding, `save_path` containment, and the mutation rule.

## 3. Wiring
- `agent.rs`: `generate_image` dispatched before the `spawn_blocking` tail (it's async and emits its own progress events); tool definitions extended; `is_mutating_call` consults `images::`.
- `config.rs`: `default_image_model` on `OmniRouteConfig` (`#[serde(default)]`, so existing configs deserialize unchanged) + `set_default_image_model`, kept as its own setter for the same reason `set_default_model` is — an image-model picker has no business round-tripping the connection fields.
- `main.rs`: `list_image_models`, `set_default_image_model`, `load_image_artifact`, `save_image_artifact`, `read_image_artifact_base64`.
- `capabilities/default.json`: `dialog:allow-save` (for the Save dialog). No asset-protocol scope needed — the cards get bytes through a command instead, which also sidesteps Windows-vs-POSIX path differences.
- `Cargo.toml`: `base64 = "0.22"`.

## 4. System prompt — `src-tauri/src/prompts.rs`
- New `IMAGE_GENERATION_ADDENDUM`, appended alongside the skill-authoring addendum in coding mode.
- Separate from the tool description because the failure mode isn't "didn't know the parameters" — it's a model answering "draw me X" with a beautifully written prompt for the user to paste into some other tool. The addendum says plainly that rendering is the expected reply, that a style skill's templates should *feed* the call rather than be the deliverable, that the prompt must be self-contained (the image model sees neither the conversation nor the attached reference), and that iteration means a fully rewritten prompt.

## 5. The in-flight animation — `src/app/ImageGenCard.tsx` + `src/styles.css`
- A frame at the requested aspect ratio holding four stacked layers: a slow counter-rotating plasma bloom (the image resolving out of nothing), a faint canvas grid, a develop-beam sweeping top to bottom the way a print comes up in a tray, drifting motes, and a rotating conic edge-light. Pure CSS transforms/opacity — no JS driving frames.
- A three-step track (Choosing model → Rendering → Saving) driven by the real backend stages, plus an elapsed-seconds counter. No percentage bar: a provider call is one opaque await, so any number would be theatre.
- Palette follows the existing two-hue rule — arcane violet while working, gold on completion (the finished artifact lands with a decaying gold edge-flash rather than a fade-in).
- Whole block collapses to one quiet pulse under `prefers-reduced-motion`.

## 6. Artifact save/copy — `src/app/ImageArtifactCard.tsx`, `src/app/imageArtifacts.ts`
- Copy / Save / Folder / Expand. Each button briefly becomes its own confirmation ("Copy" → "Copied"), which is the entire feedback mechanism — a global toast system is more machinery than three buttons warrant, and confirmation belongs on the thing you clicked.
- Copy has three tiers, because clipboard support is the least uniform thing in any webview: the async Clipboard API with a real image item, then a canvas PNG re-encode (WebKit refuses WebP), then the path as text. The label says which one happened.
- Save goes through the native dialog and a backend byte copy, so what lands on disk is exactly what the provider rendered — no canvas round-trip, no quality loss. Folder uses `revealItemInDir`, and is hidden for an artifact with no file of its own.
- A reopened session repaints from the paths parsed out of the persisted tool result (`parseArtifactPaths`), re-reading bytes off disk; files that are gone render as a one-line note instead of a broken image.
- `MessageContent.tsx` routes images attached to an assistant message through the same card, so an image the assistant replies with gets the same controls rather than being a dead `<img>`.
- Routed in both `SessionView.tsx` and `SubagentView.tsx`, so a delegated render renders properly too.

## Notes / follow-ups
- **No Settings picker for the image model.** Auto-discovery means it isn't needed to use the feature; `list_image_models` + `set_default_image_model` are there to wire a dropdown into `Settings.tsx` next to the existing model picker whenever you want one.
- **Coding mode only.** General mode has no tool list at all, so there's nothing to hang `generate_image` off — worth revisiting if image chat is a use case for that mode.
- `reflect.rs` decides whether a session "did work" via the name-only `agent::is_mutating`, which doesn't include `generate_image`. Left alone deliberately, but an image-only session currently won't trigger a reflection pass — arguably it should, given prompt craft is exactly the kind of thing worth proposing a skill about.
- `n > 1` renders variations but only the first honors `save_path`; the model is told as much in the result text.
