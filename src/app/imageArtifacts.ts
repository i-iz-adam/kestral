import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { ImageArtifact } from "../types";

/** What the artifact toolbar reports back after an action, so the button
 * can show "Copied" / "Saved" inline instead of the app needing a global
 * toast system for three buttons. */
export type ArtifactActionResult = { ok: true; label: string } | { ok: false; label: string };

const EXT_FILTERS: Record<string, { name: string; extensions: string[] }> = {
  "image/png": { name: "PNG image", extensions: ["png"] },
  "image/jpeg": { name: "JPEG image", extensions: ["jpg", "jpeg"] },
  "image/webp": { name: "WebP image", extensions: ["webp"] },
  "image/gif": { name: "GIF image", extensions: ["gif"] },
  "image/svg+xml": { name: "SVG image", extensions: ["svg"] },
};

function dataUrlToBlob(dataUrl: string, mime: string): Blob {
  const base64 = dataUrl.includes(",") ? dataUrl.slice(dataUrl.indexOf(",") + 1) : dataUrl;
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return new Blob([bytes], { type: mime });
}

/** Copies the actual image to the clipboard, so it can be pasted into
 * Discord/Photoshop/a doc rather than pasting a file path.
 *
 * Three tiers, because clipboard support is the least uniform thing in
 * any webview: the async Clipboard API with a real image item is the
 * good path; a PNG re-encode via canvas covers webviews that will only
 * accept image/png (WebKit refuses WebP, for one); and copying the path
 * as text is the honest last resort — it still gets the person
 * somewhere, and the label says which one happened.
 */
export async function copyArtifactToClipboard(
  artifact: ImageArtifact
): Promise<ArtifactActionResult> {
  const dataUrl = artifact.data_url;

  const writeBlob = async (blob: Blob) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ClipboardItemCtor = (window as any).ClipboardItem;
    if (!navigator.clipboard || typeof ClipboardItemCtor !== "function") {
      throw new Error("no clipboard image support");
    }
    await navigator.clipboard.write([new ClipboardItemCtor({ [blob.type]: blob })]);
  };

  if (dataUrl) {
    try {
      await writeBlob(dataUrlToBlob(dataUrl, artifact.mime));
      return { ok: true, label: "Copied" };
    } catch {
      // Fall through to the PNG re-encode.
    }

    try {
      const png = await reencodeAsPng(dataUrl);
      if (png) {
        await writeBlob(png);
        return { ok: true, label: "Copied" };
      }
    } catch {
      // Fall through to copying the path.
    }
  }

  try {
    await navigator.clipboard.writeText(artifact.path);
    return { ok: true, label: "Path copied" };
  } catch {
    return { ok: false, label: "Copy failed" };
  }
}

function reencodeAsPng(dataUrl: string): Promise<Blob | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = img.naturalWidth;
      canvas.height = img.naturalHeight;
      const ctx = canvas.getContext("2d");
      if (!ctx) {
        resolve(null);
        return;
      }
      ctx.drawImage(img, 0, 0);
      canvas.toBlob((blob) => resolve(blob), "image/png");
    };
    img.onerror = () => resolve(null);
    img.src = dataUrl;
  });
}

/** Native "Save as" — the dialog picks the destination, the backend does
 * the byte copy, so what lands on disk is exactly what the provider
 * rendered (no canvas round-trip, no quality loss). */
export async function saveArtifactAs(artifact: ImageArtifact): Promise<ArtifactActionResult> {
  const filter = EXT_FILTERS[artifact.mime] ?? EXT_FILTERS["image/png"];
  try {
    const destination = await save({
      title: "Save image",
      defaultPath: artifact.name,
      filters: [filter],
    });
    if (!destination) return { ok: true, label: "Save" };
    await invoke<string>("save_image_artifact", {
      source: artifact.path || null,
      dataUrl: artifact.data_url || null,
      destination,
    });
    return { ok: true, label: "Saved" };
  } catch {
    return { ok: false, label: "Save failed" };
  }
}

export async function revealArtifact(artifact: ImageArtifact): Promise<ArtifactActionResult> {
  try {
    await revealItemInDir(artifact.path);
    return { ok: true, label: "Opened" };
  } catch {
    return { ok: false, label: "Couldn't open" };
  }
}

/** Repaints artifacts for a session that was reopened after a restart:
 * the persisted tool result only kept file paths, so the bytes come back
 * off disk. Missing files (someone cleared app data) are dropped rather
 * than rendering as broken images. */
export async function loadArtifactsFromPaths(paths: string[]): Promise<ImageArtifact[]> {
  const loaded = await Promise.all(
    paths.map((path) =>
      invoke<ImageArtifact>("load_image_artifact", { path }).catch(() => null)
    )
  );
  return loaded.filter((a): a is ImageArtifact => a !== null);
}

/** Pulls the saved image paths back out of a persisted generate_image
 * tool result (the "- <path>" lines images.rs writes). Keeps them in
 * order and ignores the surrounding prose, so the format of the result
 * text can change around them without breaking rehydration. */
export function parseArtifactPaths(result: string | undefined | null): string[] {
  if (!result) return [];
  const paths: string[] = [];
  for (const line of result.split("\n")) {
    const match = /^\s*-\s+(\S.*\.(?:png|jpe?g|webp|gif|svg))\s*$/i.exec(line);
    if (match) paths.push(match[1].trim());
  }
  return paths;
}

/** Pulls the pre-edit source path out of a persisted edit_image result
 * (the `source: <path>` line). Deliberately not a `- ` bullet on the
 * backend side, so `parseArtifactPaths` never mistakes the original for
 * one of the outputs. */
export function parseSourcePath(result: string | undefined | null): string | null {
  if (!result) return null;
  for (const line of result.split("\n")) {
    const match = /^\s*source:\s+(\S.*)$/i.exec(line);
    if (match) return match[1].trim();
  }
  return null;
}

export function formatBytes(bytes: number): string {
  if (!bytes) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
