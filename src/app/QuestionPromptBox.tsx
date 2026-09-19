import React, { useState, useRef, useEffect } from "react";

interface QuestionPromptBoxProps {
  callId: string;
  question: string;
  options?: string[];
  onAnswer: (answer: string) => void;
  onDismiss: () => void;
}

export default function QuestionPromptBox({
  question,
  options,
  onAnswer,
  onDismiss,
}: QuestionPromptBoxProps) {
  const [customText, setCustomText] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleCustomSubmit = (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    const trimmed = customText.trim();
    if (!trimmed) return;
    onAnswer(trimmed);
    setCustomText("");
  };

  return (
    <div className="question-prompt-box">
      <div className="question-prompt-header">
        <div className="question-prompt-title">
          <span className="question-icon">❓</span>
          <span className="question-label font-mono">Agent Question</span>
        </div>
        <button
          type="button"
          className="question-dismiss-btn"
          onClick={onDismiss}
          title="Dismiss / Ignore question"
          aria-label="Dismiss question"
        >
          &times;
        </button>
      </div>

      <div className="question-prompt-body">
        <div className="question-text">{question}</div>

        {options && options.length > 0 && (
          <div className="question-options-grid">
            {options.map((opt, idx) => (
              <button
                key={idx}
                type="button"
                className="question-option-btn"
                onClick={() => onAnswer(opt)}
              >
                <span className="option-bullet font-mono">{idx + 1}.</span>
                <span className="option-label">{opt}</span>
              </button>
            ))}
          </div>
        )}

        <form className="question-custom-form" onSubmit={handleCustomSubmit}>
          <input
            ref={inputRef}
            type="text"
            className="question-custom-input"
            value={customText}
            onChange={(e) => setCustomText(e.target.value)}
            placeholder="Type your own answer..."
          />
          <button
            type="submit"
            className="question-submit-btn primary"
            disabled={!customText.trim()}
          >
            Submit
          </button>
        </form>
      </div>
    </div>
  );
}
