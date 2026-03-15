import type { ReactNode } from "react";

interface AppShellProps {
  onSettingsClick: () => void;
  children: ReactNode;
}

export function AppShell({ onSettingsClick, children }: AppShellProps) {
  return (
    <div className="app-shell">
      <header className="header-bar">
        <div className="header-title">
          <h1>FlacCrunch</h1>
          <span className="header-subtitle">Lossless FLAC Optimizer</span>
        </div>
        <button
          className="btn-icon"
          onClick={onSettingsClick}
          title="Settings"
          aria-label="Settings"
        >
          <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="10" cy="10" r="3" />
            <path d="M10 1.5v2M10 16.5v2M3.5 3.5l1.4 1.4M15.1 15.1l1.4 1.4M1.5 10h2M16.5 10h2M3.5 16.5l1.4-1.4M15.1 4.9l1.4-1.4" />
          </svg>
        </button>
      </header>
      <main className="content">{children}</main>
    </div>
  );
}
