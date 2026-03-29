import type { ReactNode } from "react";

interface AppShellProps {
  onSettingsClick: () => void;
  onAddFolder: () => void;
  onAddFiles: () => void;
  showQueueActions: boolean;
  children: ReactNode;
}

export function AppShell({
  onSettingsClick,
  onAddFolder,
  onAddFiles,
  showQueueActions,
  children,
}: AppShellProps) {
  return (
    <div className="win98-shell">
      <div className="window app-window">
        <div className="title-bar">
          <div className="title-bar-text">FlacCrunch</div>
        </div>
        <div className="window-body app-window-body">
          <div className="field-row app-toolbar">
            <div className="field-row app-toolbar-actions">
              {showQueueActions && (
                <>
                  <button onClick={onAddFolder}>Folder...</button>
                  <button onClick={onAddFiles}>Files...</button>
                </>
              )}
              <button onClick={onSettingsClick}>Settings...</button>
            </div>
            <span className="app-toolbar-brand">Lossless Optimizer</span>
          </div>
          <main className="content">{children}</main>
        </div>
      </div>
    </div>
  );
}
