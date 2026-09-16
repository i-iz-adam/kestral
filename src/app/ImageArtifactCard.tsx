import { useEffect, useRef, useState } from "react";
import type { ImageArtifact } from "../types";
import ImageLightboxModal from "./ImageLightboxModal";
import {
  copyArtifactToClipboard,
  formatBytes,
  revealArtifact,
  saveArtifactAs,
  type ArtifactActionResult,
} from "./imageArtifacts";

/** A button whose label briefly becomes the outcome of its own action
 * ("Copy" -> "Copied"), which is the whole feedback mechanism here. A
 * global toast system would be more machinery than three buttons
 * warrant, and confirmation belongs on the thing you clicked anyway. */
function ActionButton({
  label,
  title,
  icon,
  onRun,
}: {
  label: string;
  title: string;
  icon: JSX.Element;
  onRun: () => Promise<ArtifactActionResult>;
}) {
  const [state, setState] = useState<{ label: string; ok: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const timer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timer.current) window.clearTimeout(timer.current);
    },
    []
  );

  const run = async () => {
    if (busy) return;
    setBusy(true);
    const result = await onRun();
    setBusy(false);
    setState({ label: result.label, ok: result.ok });
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setState(null), 1800);
  };

  return (
    <button
      type="button"
      className={
        "artifact-action" +
        (busy ? " busy" : "") +
        (state ? (state.ok ? " confirmed" : " failed") : "")
      }
      onClick={run}
      title={title}
    >
      {icon}
      <span>{state ? state.label : label}</span>
    </button>
  );
}

const IconCopy = (
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <rect x="9" y="9" width="11" height="11" rx="2" />
    <path d="M5 15V5a2 2 0 0 1 2-2h8" />
  </svg>
);

const IconSave = (
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 3v12" />
    <path d="M7 11l5 5 5-5" />
    <path d="M4 20h16" />
  </svg>
);

const IconFolder = (
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z" />
  </svg>
);

const IconExpand = (
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M4 10V4h6" />
    <path d="M20 14v6h-6" />
    <path d="M4 4l6 6" />
    <path d="M20 20l-6-6" />
  </svg>
);

export default function ImageArtifactCard({
  artifact,
  caption,
  meta,
}: {
  artifact: ImageArtifact;
  /** Short label under the image — the title the model gave it. */
  caption?: string;
  /** Extra provenance line (model, size). Dimensions are measured from
   * the decoded image rather than trusted from the request, since
   * providers don't always render exactly the size they were asked for. */
  meta?: string;
}) {
  const [lightbox, setLightbox] = useState(false);
  const [dimensions, setDimensions] = useState<string | null>(null);
  const [broken, setBroken] = useState(false);

  const src = artifact.data_url;
  const details = [dimensions, artifact.mime.replace("image/", "").toUpperCase(), formatBytes(artifact.bytes)]
    .filter(Boolean)
    .join(" · ");

  if (broken) {
    return (
      <div className="image-artifact broken">
        <span className="image-artifact-broken-text">
          {artifact.name} — file is no longer on disk
        </span>
      </div>
    );
  }

  return (
    <figure className="image-artifact">
      <button
        type="button"
        className="image-artifact-frame"
        onClick={() => setLightbox(true)}
        title="Click to view full size"
      >
        <img
          src={src}
          alt={caption || artifact.name}
          onLoad={(e) => {
            const img = e.currentTarget;
            if (img.naturalWidth) setDimensions(`${img.naturalWidth}×${img.naturalHeight}`);
          }}
          onError={() => setBroken(true)}
        />
        <span className="image-artifact-sheen" aria-hidden="true" />
      </button>

      <div className="image-artifact-bar">
        <div className="image-artifact-labels">
          {caption && <figcaption className="image-artifact-caption">{caption}</figcaption>}
          <span className="image-artifact-meta">{[meta, details].filter(Boolean).join(" · ")}</span>
        </div>
        <div className="image-artifact-actions">
          <ActionButton
            label="Copy"
            title="Copy the image to the clipboard"
            icon={IconCopy}
            onRun={() => copyArtifactToClipboard(artifact)}
          />
          <ActionButton
            label="Save"
            title="Save a copy somewhere else"
            icon={IconSave}
            onRun={() => saveArtifactAs(artifact)}
          />
          {artifact.path && (
            <ActionButton
              label="Folder"
              title="Show the file on disk"
              icon={IconFolder}
              onRun={() => revealArtifact(artifact)}
            />
          )}
          <button
            type="button"
            className="artifact-action"
            onClick={() => setLightbox(true)}
            title="View full size"
          >
            {IconExpand}
            <span>Expand</span>
          </button>
        </div>
      </div>

      {lightbox && (
        <ImageLightboxModal
          src={src}
          filename={caption || artifact.name}
          originalPath={artifact.path}
          onClose={() => setLightbox(false)}
        />
      )}
    </figure>
  );
}
