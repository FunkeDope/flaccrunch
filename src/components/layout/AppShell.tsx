import type { ReactNode } from "react";

type Tab = "folders" | "processing" | "results" | "settings" | "logs";

interface AppShellProps {
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
  children: ReactNode;
}

const tabs: { id: Tab; label: string }[] = [
  { id: "folders", label: "Folders" },
  { id: "processing", label: "Processing" },
  { id: "results", label: "Results" },
  { id: "settings", label: "Settings" },
  { id: "logs", label: "Logs" },
];

export function AppShell({ activeTab, onTabChange, children }: AppShellProps) {
  return (
    <div className="app-shell">
      <nav className="sidebar">
        <div className="sidebar-header">
          <h1>FlacCrunch</h1>
          <div className="subtitle">Lossless FLAC Optimizer</div>
        </div>
        <ul className="sidebar-nav">
          {tabs.map((tab) => (
            <li key={tab.id}>
              <button
                className={activeTab === tab.id ? "active" : ""}
                onClick={() => onTabChange(tab.id)}
              >
                {tab.label}
              </button>
            </li>
          ))}
        </ul>
      </nav>
      <main className="content">{children}</main>
    </div>
  );
}
