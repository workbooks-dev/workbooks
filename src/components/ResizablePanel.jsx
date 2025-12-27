import { useState, useRef, useEffect } from "react";

/**
 * ResizablePanel - A panel that can be resized by dragging a handle
 *
 * @param {object} props
 * @param {React.ReactNode} props.children - Panel content
 * @param {number} props.defaultWidth - Default width in pixels
 * @param {number} props.minWidth - Minimum width in pixels
 * @param {number} props.maxWidth - Maximum width in pixels
 * @param {boolean} props.isCollapsed - Whether panel is collapsed
 * @param {string} props.side - Which side the resize handle is on ("left" or "right")
 * @param {string} props.storageKey - localStorage key for persisting width
 * @param {string} props.className - Additional classes for the panel
 */
export function ResizablePanel({
  children,
  defaultWidth = 300,
  minWidth = 200,
  maxWidth = 800,
  isCollapsed = false,
  side = "right",
  storageKey,
  className = "",
}) {
  // Load saved width from localStorage if available
  const getSavedWidth = () => {
    if (!storageKey) return defaultWidth;
    const saved = localStorage.getItem(storageKey);
    return saved ? parseInt(saved, 10) : defaultWidth;
  };

  const [width, setWidth] = useState(getSavedWidth());
  const [isResizing, setIsResizing] = useState(false);
  const startXRef = useRef(0);
  const startWidthRef = useRef(0);

  // Save width to localStorage when it changes
  useEffect(() => {
    if (storageKey && !isCollapsed) {
      localStorage.setItem(storageKey, width.toString());
    }
  }, [width, storageKey, isCollapsed]);

  const handleMouseDown = (e) => {
    e.preventDefault();
    setIsResizing(true);
    startXRef.current = e.clientX;
    startWidthRef.current = width;
  };

  useEffect(() => {
    const handleMouseMove = (e) => {
      if (!isResizing) return;

      const delta = side === "right" ? e.clientX - startXRef.current : startXRef.current - e.clientX;
      const newWidth = Math.min(maxWidth, Math.max(minWidth, startWidthRef.current + delta));
      setWidth(newWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
    };

    if (isResizing) {
      document.addEventListener("mousemove", handleMouseMove);
      document.addEventListener("mouseup", handleMouseUp);
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
    }

    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
  }, [isResizing, side, minWidth, maxWidth]);

  if (isCollapsed) {
    return null;
  }

  return (
    <div
      className={`relative flex-shrink-0 ${className}`}
      style={{ width: `${width}px` }}
    >
      {/* Resize handle */}
      <div
        className={`absolute top-0 bottom-0 w-1 cursor-col-resize hover:bg-blue-500 z-10 transition-colors ${
          side === "right" ? "right-0" : "left-0"
        } ${isResizing ? "bg-blue-500" : ""}`}
        onMouseDown={handleMouseDown}
        title="Drag to resize"
      >
        {/* Increase hit area */}
        <div className="absolute inset-y-0 -left-1 -right-1" />
      </div>

      {/* Panel content */}
      {children}
    </div>
  );
}
