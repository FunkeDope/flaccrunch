import type { ReactNode } from "react";
import iconUrl from "../../../src-tauri/icons/32x32.png";
import { DEFAULT_THEME, type ThemeId } from "../../themes";
import { MenuBar, type MenuDef } from "./MenuBar";
import { WindowControls } from "./WindowControls";
import { MatrixRain, WinampVis } from "./ThemeChrome";

interface AppShellProps {
  /** Shown in the app's own title bar (the native one is turned off). */
  title: string;
  menus: MenuDef[];
  /** Draw minimise / maximise / close (desktop, except macOS). */
  windowControls?: boolean;
  /** A run is in progress; drives the Winamp visualiser. */
  busy?: boolean;
  /** Active display theme; drives the decorative chrome some themes add. */
  theme?: ThemeId;
  children: ReactNode;
}

/**
 * The window frame: one title bar holding the icon, title, menu bar and window
 * buttons. The empty parts of the bar drag the window (double-click
 * maximises). Themes decide the layout in CSS: base keeps it on one row, the
 * classic Windows themes wrap the menu bar onto its own row under the title.
 */
export function AppShell({
  title,
  menus,
  windowControls = false,
  busy = false,
  theme = DEFAULT_THEME,
  children,
}: AppShellProps) {
  return (
    <div className="app-shell">
      {theme === "matrix" && <MatrixRain />}
      <div className="window app-window">
        <header className="title-bar app-title-bar" data-tauri-drag-region>
          <img className="app-title-icon" src={iconUrl} alt="" draggable={false} data-tauri-drag-region />
          <div className="title-bar-text" data-tauri-drag-region>{title}</div>
          <MenuBar menus={menus} />
          <div className="app-title-drag" data-tauri-drag-region />
          {theme === "winamp" && <WinampVis active={busy} />}
          {windowControls && <WindowControls />}
        </header>
        <div className="window-body app-window-body">
          <main className="content">{children}</main>
        </div>
      </div>
    </div>
  );
}
