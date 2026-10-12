import { useEffect, useState } from "react";
import { THEMES, type ThemeId } from "../../themes";

interface ThemeMenuProps {
  theme: ThemeId;
  onSelect: (id: ThemeId) => void;
}

/**
 * Floating theme switcher. Fixed to the bottom-left corner and styled by
 * unlayered CSS in App.css, so it looks and sits the same in every theme.
 * Toggle with the button or Ctrl+Shift+T; Escape closes it.
 */
export function ThemeMenu({ theme, onSelect }: ThemeMenuProps) {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "t") {
        event.preventDefault();
        setOpen((value) => !value);
      } else if (event.key === "Escape") {
        setOpen(false);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const current = THEMES.find((t) => t.id === theme);

  return (
    <div className={`theme-menu ${open ? "is-open" : ""}`}>
      {open && (
        <div className="theme-menu-panel" role="menu" aria-label="Display theme">
          <div className="theme-menu-title">Display theme</div>
          {THEMES.map((t) => (
            <button
              key={t.id}
              type="button"
              role="menuitemradio"
              aria-checked={t.id === theme}
              className={`theme-menu-item ${t.id === theme ? "is-active" : ""}`}
              onClick={() => onSelect(t.id)}
              title={t.tagline}
            >
              <span className="theme-menu-dot" aria-hidden="true" />
              {t.name}
            </button>
          ))}
          <div className="theme-menu-hint">Ctrl+Shift+T toggles this menu</div>
        </div>
      )}
      <button
        type="button"
        className="theme-menu-toggle"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        title="Switch display theme (Ctrl+Shift+T)"
      >
        <span className="theme-menu-toggle-label">{current?.name ?? "Theme"}</span>
      </button>
    </div>
  );
}
