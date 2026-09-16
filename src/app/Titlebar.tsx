import { useEffect, useState, MouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

export default function Titlebar() {
  const appWindow = getCurrentWindow();
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const checkMaximized = async () => {
      try {
        const maximized = await appWindow.isMaximized();
        setIsMaximized(maximized);
      } catch (e) {
        // Safe fallback outside Tauri context
      }
    };

    checkMaximized();

    const setupListener = async () => {
      try {
        unlisten = await appWindow.onResized(async () => {
          checkMaximized();
        });
      } catch (e) {
        // Ignore outside Tauri
      }
    };

    setupListener();

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const handleMinimize = async () => {
    try {
      await appWindow.minimize();
    } catch (e) {
      console.warn("Minimize not available outside Tauri:", e);
    }
  };

  const handleMaximize = async () => {
    try {
      await appWindow.toggleMaximize();
      const maximized = await appWindow.isMaximized();
      setIsMaximized(maximized);
    } catch (e) {
      console.warn("Maximize not available outside Tauri:", e);
    }
  };

  const handleClose = async () => {
    try {
      await appWindow.close();
    } catch (e) {
      console.warn("Close not available outside Tauri:", e);
    }
  };

  const handleMouseDown = async (e: MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest(".titlebar-controls")) return;
    if (e.button === 0 && e.detail === 1) {
      try {
        await appWindow.startDragging();
      } catch (err) {
        // Safe fallback outside Tauri context
      }
    }
  };

  const handleDoubleClick = async (e: MouseEvent) => {
    if (e.button === 0) {
      handleMaximize();
    }
  };

  return (
    <header
      className="titlebar"
      data-tauri-drag-region
      onMouseDown={handleMouseDown}
      onDoubleClick={handleDoubleClick}
    >
      <div className="titlebar-brand" data-tauri-drag-region>
        <svg
          className="titlebar-logo"
          width="18"
          height="18"
          viewBox="0 0 100 100"
          fill="none"
          xmlns="http://www.w3.org/2000/svg"
        >
          <polygon points="30,78 97.5,72.5 90.5,47.5" fill="#9b6bff" fillOpacity="0.6" />
          <polygon points="30,78 91.0,51.2 79.0,32.8" fill="#9b6bff" fillOpacity="0.9" />
          <polygon points="30,78 79.2,27.4 64.8,16.6" fill="#e0b45c" />
        </svg>
        <span className="titlebar-title" data-tauri-drag-region>
          Kestrel
        </span>
      </div>

      <div className="titlebar-center" data-tauri-drag-region />

      <div
        className="titlebar-controls"
        onMouseDown={(e) => e.stopPropagation()}
        onDoubleClick={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          className="titlebar-btn minimize"
          onClick={handleMinimize}
          title="Minimize"
          aria-label="Minimize Window"
        >
          <svg className="btn-icon" width="12" height="12" viewBox="0 0 12 12" fill="none">
            <rect x="2" y="5.5" width="8" height="1.2" rx="0.6" fill="currentColor" />
          </svg>
        </button>

        <button
          type="button"
          className="titlebar-btn maximize"
          onClick={handleMaximize}
          title={isMaximized ? "Restore" : "Maximize"}
          aria-label={isMaximized ? "Restore Window" : "Maximize Window"}
        >
          {isMaximized ? (
            <svg className="btn-icon" width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path
                d="M4 2.5H9.5V8"
                stroke="currentColor"
                strokeWidth="1.2"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
              <rect
                x="2.5"
                y="4"
                width="5.5"
                height="5.5"
                rx="0.75"
                stroke="currentColor"
                strokeWidth="1.2"
                fill="none"
              />
            </svg>
          ) : (
            <svg className="btn-icon" width="12" height="12" viewBox="0 0 12 12" fill="none">
              <rect
                x="2.5"
                y="2.5"
                width="7"
                height="7"
                rx="1"
                stroke="currentColor"
                strokeWidth="1.2"
                fill="none"
              />
            </svg>
          )}
        </button>

        <button
          type="button"
          className="titlebar-btn close"
          onClick={handleClose}
          title="Close"
          aria-label="Close Window"
        >
          <svg className="btn-icon" width="12" height="12" viewBox="0 0 12 12" fill="none">
            <path
              d="M2.5 2.5L9.5 9.5M9.5 2.5L2.5 9.5"
              stroke="currentColor"
              strokeWidth="1.3"
              strokeLinecap="round"
            />
          </svg>
        </button>
      </div>
    </header>
  );
}
