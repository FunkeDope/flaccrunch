import type { ReactNode } from "react";
import { DEFAULT_THEME, type ThemeId } from "../../themes";
import { MatrixRain, WinampVis } from "./ThemeChrome";

interface AppShellProps {
  onSettingsClick: () => void;
  onAddFolder: () => void;
  onAddFiles: () => void;
  showQueueActions: boolean;
  /** Active display theme; drives the decorative chrome some themes add. */
  theme?: ThemeId;
  children: ReactNode;
}

export function AppShell({
  onSettingsClick,
  onAddFolder,
  onAddFiles,
  showQueueActions,
  theme = DEFAULT_THEME,
  children,
}: AppShellProps) {
  const busy = !showQueueActions;

  return (
    <div className="app-shell">
      {theme === "matrix" && <MatrixRain />}
      <div className="window app-window">
        <div className="title-bar app-title-bar">
          <div className="title-bar-text">FlacCrunch</div>
          {theme === "winamp" && <WinampVis active={busy} />}
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
            <div className="app-toolbar-right">
              <span className="app-toolbar-brand">Lossless Optimizer</span>
            </div>
          </div>
          <main className="content">{children}</main>
        </div>
      </div>
    </div>
  );
}
