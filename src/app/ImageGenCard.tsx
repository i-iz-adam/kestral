import { useEffect, useMemo, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ImageArtifact,
  ImageGenMode,
  ImageGenStage,
  ImageProgressEventPayload,
  ImageReadyEventPayload,
  ToolCallEventPayload,
} from "../types";
import ImageArtifactCard from "./ImageArtifactCard";
import {
  loadArtifactsFromPaths,
  parseArtifactPaths,
  parseSourcePath,
} from "./imageArtifacts";

/** The stages shown as a little three-step track while a render is in
 * flight. Deliberately not a percentage: a provider call is one opaque
 * await, so any number would be theatre. The stages are real (we know
 * which one we're in), and the animation supplies the motion. */
const STAGE_TRACK: { id: ImageGenStage; label: string }[] = [
  { id: "resolving", label: "Choosing model" },
  { id: "rendering", label: "Rendering" },
  { id: "saving", label: "Saving" },
];

/** The edit path's first stage is finding the image to work on, which is
 * the step most likely to fail (and the one worth naming, since "which
 * image?" is the question a user would ask). */
const EDIT_STAGE_TRACK: { id: ImageGenStage; label: string }[] = [
  { id: "resolving", label: "Finding source" },
  { id: "rendering", label: "Editing" },
  { id: "saving", label: "Saving" },
];

const STAGE_ORDER: Record<ImageGenStage, number> = {
  resolving: 0,
  dispatched: 0,
  rendering: 1,
  saving: 2,
  done: 3,
  error: 3,
};

function aspectFromSize(size: string | undefined): number {
  if (!size) return 1;
  const match = /^(\d+)\s*[x×]\s*(\d+)$/i.exec(size.trim());
  if (!match) return 1;
  const w = Number(match[1]);
  const h = Number(match[2]);
  if (!w || !h) return 1;
  return w / h;
}

/** The placeholder that plays while the image is being generated: a
 * frame at the requested aspect ratio containing a slow plasma bloom (the
 * image "forming"), a develop-beam that sweeps top to bottom, drifting
 * motes, and a rotating conic glow on the border. All CSS — see
 * styles.css, `.image-forge-*` — so it costs nothing to run and respects
 * prefers-reduced-motion. */
function ForgeFrame({
  aspect,
  stage,
  mode,
  sourceDataUrl,
  model,
  prompt,
  elapsed,
}: {
  aspect: number;
  stage: ImageGenStage;
  mode: ImageGenMode;
  sourceDataUrl?: string | null;
  model?: string | null;
  prompt?: string | null;
  elapsed: number;
}) {
  const activeIndex = STAGE_ORDER[stage] ?? 0;
  const editing = mode === "edit";
  const track = editing ? EDIT_STAGE_TRACK : STAGE_TRACK;

  return (
    <div className="image-forge">
      <div
        className={"image-forge-frame" + (editing && sourceDataUrl ? " editing" : "")}
        // An edit keeps the source's own proportions; only a generation
        // has a requested size to shape the frame by.
        style={editing && sourceDataUrl ? undefined : { aspectRatio: String(aspect) }}
        role="img"
        aria-label={editing ? "Editing image" : "Generating image"}
      >
        {/* During an edit the source sits under the effects, so what you
            watch is this image being worked on rather than an empty box —
            the whole point of the edit path is that it's the same
            picture, and the animation should say so. */}
        {editing && sourceDataUrl && (
          <img className="image-forge-source" src={sourceDataUrl} alt="" aria-hidden="true" />
        )}
        <span className="image-forge-plasma" aria-hidden="true" />
        <span className="image-forge-grid" aria-hidden="true" />
        <span className="image-forge-beam" aria-hidden="true" />
        <span className="image-forge-motes" aria-hidden="true">
          {Array.from({ length: 7 }).map((_, i) => (
            <i key={i} style={{ ["--i" as string]: String(i) }} />
          ))}
        </span>
        <span className="image-forge-ring" aria-hidden="true" />
      </div>

      <div className="image-forge-status">
        <div className="image-forge-track">
          {track.map((s, idx) => (
            <span
              key={s.id}
              className={
                "image-forge-step" +
                (idx < activeIndex ? " complete" : "") +
                (idx === activeIndex ? " active" : "")
              }
            >
              <i aria-hidden="true" />
              {s.label}
            </span>
          ))}
        </div>
        <div className="image-forge-meta">
          {model && <span className="image-forge-model">{model}</span>}
          {elapsed > 0 && <span className="image-forge-elapsed">{elapsed}s</span>}
        </div>
        {prompt && <p className="image-forge-prompt">{prompt}</p>}
      </div>
    </div>
  );
}

export default function ImageGenCard({
  event,
  onApprove,
}: {
  event: ToolCallEventPayload;
  onApprove: (callId: string, approved: boolean) => void;
}) {
  const args = (event.args ?? {}) as {
    prompt?: string;
    title?: string;
    size?: string;
    model?: string;
    n?: number;
    source?: string;
    mask?: string;
  };

  // The tool name is the mode: known before any event arrives, which
  // matters for a reopened session (no live events will ever come) and
  // for the first paint of a live one.
  const mode: ImageGenMode = event.name === "edit_image" ? "edit" : "generate";

  const [stage, setStage] = useState<ImageGenStage>(() =>
    event.status === "done" ? "done" : event.status === "error" ? "error" : "resolving"
  );
  const [model, setModel] = useState<string | null>(args.model ?? null);
  const [artifacts, setArtifacts] = useState<ImageArtifact[]>([]);
  const [failure, setFailure] = useState<string | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const [sourceDataUrl, setSourceDataUrl] = useState<string | null>(null);

  const aspect = useMemo(() => aspectFromSize(args.size), [args.size]);
  const caption = args.title || args.prompt || (mode === "edit" ? "Edited image" : "Generated image");

  // Both events are per-call, so this card only listens for its own
  // call_id. Keeping it local (rather than threading image state through
  // the global agent store) means a render in a background session still
  // completes and still repaints correctly when you come back to it —
  // the artifacts are on disk and the tool result holds their paths.
  useEffect(() => {
    let unlistenProgress: UnlistenFn | undefined;
    let unlistenReady: UnlistenFn | undefined;
    let cancelled = false;

    listen<ImageProgressEventPayload>("agent://image-progress", (evt) => {
      if (evt.payload.call_id !== event.call_id) return;
      setStage(evt.payload.stage);
      if (evt.payload.model) setModel(evt.payload.model);
      if (evt.payload.source_data_url) setSourceDataUrl(evt.payload.source_data_url);
      if (evt.payload.stage === "error") {
        setFailure(
          evt.payload.message ??
            (mode === "edit" ? "Image edit failed" : "Image generation failed")
        );
      }
    }).then((un) => {
      if (cancelled) un();
      else unlistenProgress = un;
    });

    listen<ImageReadyEventPayload>("agent://image-ready", (evt) => {
      if (evt.payload.call_id !== event.call_id) return;
      setArtifacts(evt.payload.images);
      setModel(evt.payload.model);
      if (evt.payload.source_data_url) setSourceDataUrl(evt.payload.source_data_url);
      setStage("done");
    }).then((un) => {
      if (cancelled) un();
      else unlistenReady = un;
    });

    return () => {
      cancelled = true;
      unlistenProgress?.();
      unlistenReady?.();
    };
  }, [event.call_id, mode]);

  // Rehydration path: a session reopened after a restart has the tool
  // result (which carries the saved paths) but never saw the live
  // `image-ready` event, so the bytes are re-read off disk.
  useEffect(() => {
    if (artifacts.length > 0) return;
    if (event.status !== "done" || !event.result) return;
    const paths = parseArtifactPaths(event.result);
    if (paths.length === 0) return;
    let cancelled = false;
    loadArtifactsFromPaths(paths).then((loaded) => {
      if (!cancelled && loaded.length > 0) {
        setArtifacts(loaded);
        setStage("done");
      }
    });
    return () => {
      cancelled = true;
    };
  }, [event.status, event.result, artifacts.length]);

  // Same rehydration story for the "before" image: an edit's source was
  // copied into the artifact directory precisely so the pair survives a
  // restart.
  useEffect(() => {
    if (mode !== "edit" || sourceDataUrl) return;
    if (event.status !== "done" || !event.result) return;
    const path = parseSourcePath(event.result);
    if (!path) return;
    let cancelled = false;
    loadArtifactsFromPaths([path]).then((loaded) => {
      if (!cancelled && loaded.length > 0) setSourceDataUrl(loaded[0].data_url);
    });
    return () => {
      cancelled = true;
    };
  }, [mode, sourceDataUrl, event.status, event.result]);

  useEffect(() => {
    if (event.status === "error" && !failure) {
      setStage("error");
      setFailure(
        event.result ?? (mode === "edit" ? "Image edit failed" : "Image generation failed")
      );
    }
  }, [event.status, event.result, failure, mode]);

  const inFlight = stage !== "done" && stage !== "error" && event.status !== "error";

  useEffect(() => {
    if (!inFlight) return;
    const started = Date.now();
    const id = window.setInterval(() => {
      setElapsed(Math.round((Date.now() - started) / 1000));
    }, 1000);
    return () => window.clearInterval(id);
  }, [inFlight]);

  const awaiting = event.status === "awaiting-approval";

  return (
    <div className={"image-gen-card " + (inFlight ? "working" : stage)}>
      <div className="image-gen-head">
        <span className="image-gen-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
            <rect x="3" y="4" width="18" height="16" rx="2" />
            <circle cx="8.5" cy="9.5" r="1.6" />
            <path d="M21 16l-5-5-6 6" />
          </svg>
        </span>
        <span className="image-gen-label">
          {inFlight
            ? mode === "edit"
              ? "Editing image"
              : "Generating image"
            : stage === "error"
              ? mode === "edit"
                ? "Image edit failed"
                : "Image generation failed"
              : mode === "edit"
                ? "Edited image"
                : "Image"}
        </span>
        {args.n && args.n > 1 && <span className="image-gen-count">×{args.n}</span>}
        {args.size && <span className="image-gen-size">{args.size}</span>}
        {mode === "edit" && args.mask && <span className="image-gen-size">masked</span>}
      </div>

      {awaiting && (
        <div className="image-gen-approve">
          <span>
              {mode === "edit" ? "This edit" : "This render"} also writes into the
              workspace.
            </span>
          <div className="tool-approve">
            <button onClick={() => onApprove(event.call_id, true)}>Approve</button>
            <button onClick={() => onApprove(event.call_id, false)}>Reject</button>
          </div>
        </div>
      )}

      {inFlight && !awaiting && (
        <ForgeFrame
          aspect={aspect}
          stage={stage}
          mode={mode}
          sourceDataUrl={sourceDataUrl}
          model={model}
          prompt={args.prompt}
          elapsed={elapsed}
        />
      )}

      {stage === "error" && (
        <p className="image-gen-error">{failure}</p>
      )}

      {artifacts.length > 0 && (
        <div className="image-gen-outcome">
          {/* Before/after, because the question a person actually has
              about an edit is "what changed?" — and the source is already
              in hand, so showing it costs nothing. Only the result gets
              the artifact toolbar: the source is context, not a new
              artifact to save. */}
          {mode === "edit" && sourceDataUrl && (
            <div className="image-edit-source">
              <span className="image-edit-source-label">Before</span>
              <img src={sourceDataUrl} alt="Source image" />
            </div>
          )}
          <div
            className={
              "image-gen-results" +
              (artifacts.length > 1 ? " multi" : "") +
              (mode === "edit" && sourceDataUrl ? " after" : "")
            }
          >
            {mode === "edit" && sourceDataUrl && (
              <span className="image-edit-after-label">After</span>
            )}
            {artifacts.map((artifact, idx) => (
              <ImageArtifactCard
                key={artifact.path}
                artifact={artifact}
                caption={artifacts.length > 1 ? `${caption} (${idx + 1})` : caption}
                meta={[model, mode === "edit" ? "edited" : args.size]
                  .filter(Boolean)
                  .join(" · ")}
              />
            ))}
          </div>
        </div>
      )}

      {artifacts.length > 0 && artifacts[0].revised_prompt && (
        <p className="image-gen-revised">
          Provider rewrote the prompt: {artifacts[0].revised_prompt}
        </p>
      )}
    </div>
  );
}
