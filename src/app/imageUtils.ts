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

  // 1. Match base64 data URIs
  const dataUriRegex = /data:image\/(png|jpe?g|webp|svg\+xml);base64,[A-Za-z0-9+/=]+/g;
  let match: RegExpExecArray | null;
  while ((match = dataUriRegex.exec(text)) !== null) {
    const src = match[0];
    if (!seenSrcs.has(src)) {
      seenSrcs.add(src);
      images.push({
        src,
        name: `Image (${images.length + 1})`,
      });
    }
  }

  // 2. Match file paths or URLs ending in image extensions
  const filePathRegex = /(?:[a-zA-Z]:\\|\/|\.\/|\.\.\/)?[\w\-./\\]+\.(png|jpe?g|webp|svg)\b/gi;
  while ((match = filePathRegex.exec(text)) !== null) {
    const rawPath = match[0];
    if (rawPath.startsWith("data:")) continue;

    let src = rawPath;
    try {
      if (!rawPath.startsWith("http://") && !rawPath.startsWith("https://")) {
        src = convertFileSrc(rawPath);
      }
    } catch {
      src = rawPath;
    }

    if (!seenSrcs.has(src)) {
      seenSrcs.add(src);
      const filename = rawPath.split(/[/\\]/).pop() || rawPath;
      images.push({
        src,
        originalPath: rawPath,
        name: filename,
      });
    }
  }

  return images;
}
