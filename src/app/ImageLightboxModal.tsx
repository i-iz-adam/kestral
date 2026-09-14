import { useState, useEffect } from "react";

interface ImageLightboxModalProps {
  src: string;
  filename: string;
  originalPath?: string;
  onClose: () => void;
}

export default function ImageLightboxModal({
  src,
  filename,
  originalPath,
  onClose,
}: ImageLightboxModalProps) {
  const [zoom, setZoom] = useState(1);
  const [bgMode, setBgMode] = useState<"dark" | "light" | "grid">("dark");

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  return (
    <div className="lightbox-overlay" onClick={onClose}>
      <div className="lightbox-container" onClick={(e) => e.stopPropagation()}>
        <div className="lightbox-header">
          <div className="lightbox-title-group">
            <span className="lightbox-filename">{filename}</span>
            {originalPath && <span className="lightbox-path">{originalPath}</span>}
          </div>
          <div className="lightbox-controls">
            <div className="lightbox-bg-picker">
              <button
                type="button"
                className={`lightbox-btn ${bgMode === "dark" ? "active" : ""}`}
                onClick={() => setBgMode("dark")}
              >
                Dark
              </button>
              <button
                type="button"
                className={`lightbox-btn ${bgMode === "light" ? "active" : ""}`}
                onClick={() => setBgMode("light")}
              >
                Light
              </button>
              <button
                type="button"
                className={`lightbox-btn ${bgMode === "grid" ? "active" : ""}`}
                onClick={() => setBgMode("grid")}
              >
                Grid
              </button>
            </div>

            <div className="lightbox-zoom-controls">
              <button
                type="button"
                className="lightbox-btn"
                onClick={() => setZoom((z) => Math.max(0.25, z - 0.25))}
              >
                -
              </button>
              <span className="lightbox-zoom-readout">{Math.round(zoom * 100)}%</span>
              <button
                type="button"
                className="lightbox-btn"
                onClick={() => setZoom((z) => Math.min(4, z + 0.25))}
              >
                +
              </button>
              <button type="button" className="lightbox-btn" onClick={() => setZoom(1)}>
                Reset
              </button>
            </div>

            <button type="button" className="lightbox-close" onClick={onClose} title="Close (Esc)">
              ✕
            </button>
          </div>
        </div>

        <div className={`lightbox-body bg-${bgMode}`}>
          <img
            src={src}
            alt={filename}
            style={{ transform: `scale(${zoom})`, transition: "transform 0.15s ease-out" }}
          />
        </div>
      </div>
    </div>
  );
}
