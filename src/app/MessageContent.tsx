import { useEffect, useState } from "react";
import Markdown from "./Markdown";

interface ThinkSplit {
  thinking: string;
  thinkingOpen: boolean;
  answer: string;
}

// Some models emit their chain-of-thought inline as literal <think>...</think>
// text rather than as a separate reasoning field, which — left alone —
// renders as a wall of raw tags mixed into the reply. This pulls any
// complete <think> blocks out into `thinking`, and also handles the
// mid-stream case where an opening <think> has arrived but its closing
// tag hasn't yet, so the reasoning-in-progress can still be shown as such
// instead of momentarily looking like part of the answer.
function splitThinking(content: string): ThinkSplit {
  const closedRe = /<think>([\s\S]*?)<\/think>/g;
  let thinking = "";
  let lastIndex = 0;
  let strippedAnswer = "";
  let match: RegExpExecArray | null;
  while ((match = closedRe.exec(content))) {
    thinking += match[1];
    strippedAnswer += content.slice(lastIndex, match.index);
    lastIndex = closedRe.lastIndex;
  }
  strippedAnswer += content.slice(lastIndex);

  const openIdx = strippedAnswer.indexOf("<think>");
  let thinkingOpen = false;
  if (openIdx !== -1) {
    thinkingOpen = true;
    thinking += strippedAnswer.slice(openIdx + "<think>".length);
    strippedAnswer = strippedAnswer.slice(0, openIdx);
  }

  return { thinking: thinking.trim(), thinkingOpen, answer: strippedAnswer.trim() };
}

export default function MessageContent({
  role,
  content,
  images,
  streaming,
}: {
  role: string;
  content: string;
  images?: string[];
  streaming?: boolean;
}) {
  const split = role === "assistant"
    ? splitThinking(content)
    : { thinking: "", thinkingOpen: false, answer: content };
  const { thinking, thinkingOpen, answer } = split;

  const [expanded, setExpanded] = useState(thinkingOpen);

  // Once real answer text starts arriving, the reasoning phase is over —
  // collapse it automatically so the reply isn't buried under a wall of
  // thinking the person didn't ask to read (they can still reopen it).
  useEffect(() => {
    if (!thinkingOpen && answer) setExpanded(false);
  }, [thinkingOpen, answer]);

  if (role !== "assistant") {
    return (
      <div className="message-bubble-user-content">
        {images && images.length > 0 && (
          <div className="message-images-grid">
            {images.map((img, idx) => (
              <img
                key={idx}
                src={img}
                alt={`attached-${idx}`}
                className="message-attached-image"
                onError={(e) => {
                  (e.currentTarget as HTMLElement).style.display = "none";
                }}
              />
            ))}
          </div>
        )}
        {content && <p className="plain-text">{content}</p>}
      </div>
    );
  }

  return (
    <>
      {thinking && (
        <div className={"thinking-block" + (thinkingOpen ? " active" : "")}>
          <button type="button" className="thinking-toggle" onClick={() => setExpanded((v) => !v)}>
            <span className="thinking-chevron">{expanded ? "▾" : "▸"}</span>
            {thinkingOpen ? "Thinking…" : "Thoughts"}
          </button>
          {expanded && (
            <pre className={"thinking-text" + (thinkingOpen && streaming ? " caret" : "")}>
              {thinking}
            </pre>
          )}
        </div>
      )}
      {answer && <Markdown content={answer} caret={streaming && !thinkingOpen} />}
      {streaming && !answer && !thinkingOpen && (
        <p className="markdown-placeholder caret">&nbsp;</p>
      )}
    </>
  );
}
