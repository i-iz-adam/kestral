import { useMemo, type MouseEvent } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { open } from "@tauri-apps/api/shell";
import { convertFileSrc } from "@tauri-apps/api/tauri";

marked.setOptions({
  breaks: true,
  gfm: true,
});

// Tool/file-path-shaped strings from the agent (`src/foo.rs`, `**bold**`,
// fenced code, etc.) are the whole reason this exists — plain <p> text
// left every backtick and asterisk visible instead of turning them into
// the code/emphasis they were meant to be. marked.parse is synchronous
// under our options; DOMPurify strips anything that isn't safe to inject
// (scripts, event handlers, etc.) before it ever reaches the DOM.
export default function Markdown({ content, caret }: { content: string; caret?: boolean }) {
  const html = useMemo(() => {
    if (!content) return "";
    const raw = marked.parse(content, { async: false }) as string;
    let sanitized = DOMPurify.sanitize(raw, {
      ALLOWED_TAGS: [
        "p", "br", "hr",
        "strong", "em", "del", "code", "pre", "blockquote",
        "ul", "ol", "li",
        "h1", "h2", "h3", "h4", "h5", "h6",
        "a", "table", "thead", "tbody", "tr", "th", "td", "span", "img",
      ],
      ALLOWED_ATTR: ["href", "title", "class", "src", "alt"],
    });

    // Remove empty paragraphs left by stripped tags or empty lines
    sanitized = sanitized.replace(/<p>\s*<\/p>/gi, "");

    // Process img tags to convert local file paths and attach onerror fallbacks
    const dom = new DOMParser().parseFromString(sanitized, "text/html");
    const imgs = dom.querySelectorAll("img");
    if (imgs.length > 0) {
      imgs.forEach((img) => {
        const src = img.getAttribute("src");
        if (
          src &&
          !src.startsWith("data:") &&
          !src.startsWith("http://") &&
          !src.startsWith("https://") &&
          !src.startsWith("asset://")
        ) {
          try {
            img.setAttribute("src", convertFileSrc(src));
          } catch {
            // Keep original if convertFileSrc fails
          }
        }
        img.setAttribute(
          "onerror",
          "this.style.display='none';if(this.parentElement&&this.parentElement.tagName==='P'&&!this.parentElement.textContent.trim()){this.parentElement.style.display='none';}"
        );
      });
      return dom.body.innerHTML;
    }

    return sanitized;
  }, [content]);

  // Links should open in the person's actual browser, not navigate the
  // app's own webview away from the chat — this is the desktop-app
  // equivalent of target="_blank", done via Tauri's shell API instead of
  // an anchor attribute the webview would otherwise just follow in place.
  const onClick = (e: MouseEvent<HTMLDivElement>) => {
    const anchor = (e.target as HTMLElement).closest("a");
    if (anchor?.href) {
      e.preventDefault();
      open(anchor.href);
    }
  };

  if (!html) return null;
  return (
    <div
      className={"markdown" + (caret ? " caret" : "")}
      onClick={onClick}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
