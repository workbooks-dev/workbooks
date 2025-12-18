import { useEffect, useRef } from "react";

export function ContextMenu({ x, y, items, onClose }) {
  const menuRef = useRef(null);

  useEffect(() => {
    const handleClickOutside = (e) => {
      if (menuRef.current && !menuRef.current.contains(e.target)) {
        onClose();
      }
    };

    const handleEscape = (e) => {
      if (e.key === "Escape") {
        onClose();
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleEscape);

    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [onClose]);

  // Adjust position to keep menu on screen
  const adjustedStyle = {
    left: `${x}px`,
    top: `${y}px`,
  };

  return (
    <div
      ref={menuRef}
      className="context-menu"
      style={adjustedStyle}
      onClick={(e) => e.stopPropagation()}
    >
      {items.map((item, index) => (
        <div
          key={index}
          className="context-menu-item"
          onClick={() => {
            item.action();
            onClose();
          }}
        >
          {item.label}
        </div>
      ))}
    </div>
  );
}
