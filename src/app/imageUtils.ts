import { convertFileSrc } from "@tauri-apps/api/tauri";

export interface ExtractedImage {
  src: string;
  originalPath?: string;
  name: string;
}

export function extractImagesFromText(text: string): ExtractedImage[] {
  if (!text) return [];

  const images: ExtractedImage[] = [];
  const seenSrcs = new Set<string>();

  const addImage = (rawPath: string, defaultName?: string) => {
    const path = rawPath.trim();
    if (!path) return;

    let src = path;
    if (!path.startsWith("data:") && !path.startsWith("http://") && !path.startsWith("https://")) {
      try {
        src = convertFileSrc(path);
      } catch {
        src = path;
      }
    }

    if (!seenSrcs.has(src)) {
      seenSrcs.add(src);
      const filename = defaultName || path.split(/[/\\]/).pop() || path;
      images.push({
        src,
        originalPath: path,
        name: filename,
      });
    }
  };

  // 1. Match base64 data URIs
  const dataUriRegex = /data:image\/(png|jpe?g|webp|svg\+xml);base64,[A-Za-z0-9+/=]+/g;
  let match: RegExpExecArray | null;
  while ((match = dataUriRegex.exec(text)) !== null) {
    addImage(match[0], `Image (${images.length + 1})`);
  }

  // 2. Match explicitly tagged generated artifacts (e.g. "- /path/to/image.png")
  const artifactRegex = /^\s*-\s*([^\s\n]+\.(png|jpe?g|webp|svg))\b/gim;
  while ((match = artifactRegex.exec(text)) !== null) {
    if (!match[1].startsWith("data:")) {
      addImage(match[1]);
    }
  }

  // 3. Match Markdown image syntax ![alt](path)
  const markdownImgRegex = /!\[.*?\]\(([^)\s]+)\)/g;
  while ((match = markdownImgRegex.exec(text)) !== null) {
    if (!match[1].startsWith("data:")) {
      addImage(match[1]);
    }
  }

  // 4. Match absolute file paths or web URLs ending in image extensions
  const absoluteOrUrlRegex = /(?:https?:\/\/|file:\/\/|\/|[a-zA-Z]:[/\\])[^\s\n<>()"']+\.(png|jpe?g|webp|svg)\b/gi;
  while ((match = absoluteOrUrlRegex.exec(text)) !== null) {
    if (!match[0].startsWith("data:")) {
      addImage(match[0]);
    }
  }

  return images;
}
