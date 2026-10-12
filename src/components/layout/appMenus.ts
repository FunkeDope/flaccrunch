import type { MenuDef, MenuEntry } from "./MenuBar";
import { THEMES, type ThemeId } from "../../themes";
import type { AppSettings } from "../../types/settings";
import type { RunStatus } from "../../types/processing";
import type { ExternalTarget } from "../../lib/tauri";

/** Everything the menus can do; App and the preview harness both provide it. */
export interface MenuActions {
  status: RunStatus;
  canStart: boolean;
  /** Desktop app: show Exit, folder and browser items. */
  desktop: boolean;
  settings: AppSettings;
  onUpdate: (partial: Partial<AppSettings>) => void;
  onAddFolder: () => void;
  onAddFiles: () => void;
  onStart: () => void;
  onCancel: () => void;
  onNewRun: () => void;
  onShowLog: () => void;
  onExportLog: () => void;
  onPreferences: () => void;
  onTheme: (id: ThemeId) => void;
  onNextTheme: () => void;
  onAbout: () => void;
  onOpen: (target: ExternalTarget) => void;
  onReload: () => void;
  onToggleDevtools: () => void;
  onExit: () => void;
}

function themeItems(current: ThemeId, onTheme: (id: ThemeId) => void): MenuEntry[] {
  return THEMES.map((t) => ({
    label: t.name,
    checked: t.id === current,
    radio: true,
    onSelect: () => onTheme(t.id),
  }));
}

export function buildMenus(a: MenuActions): MenuDef[] {
  const running = a.status === "processing" || a.status === "cancelling";
  const complete = a.status === "complete";
  const idle = a.status === "idle";
  const s = a.settings;
  const toggle = (key: keyof AppSettings) => () => a.onUpdate({ [key]: !s[key] } as Partial<AppSettings>);

  return [
    {
      id: "file",
      label: "&File",
      items: [
        { label: "Add &Folder…", shortcut: "Ctrl+O", onSelect: a.onAddFolder, disabled: !idle },
        { label: "Add F&iles…", shortcut: "Ctrl+Shift+O", onSelect: a.onAddFiles, disabled: !idle },
        { type: "separator" },
        { label: "&Start", shortcut: "Ctrl+Enter", onSelect: a.onStart, disabled: !a.canStart },
        { label: "&Cancel Run", onSelect: a.onCancel, disabled: a.status !== "processing" },
        { label: "&New Run", onSelect: a.onNewRun, disabled: !complete },
        { type: "separator" },
        { label: "Show Full &Log…", shortcut: "Ctrl+L", onSelect: a.onShowLog, disabled: !complete },
        { label: "&Export Log…", shortcut: "Ctrl+S", onSelect: a.onExportLog, disabled: !complete },
        { type: "separator", hidden: !a.desktop },
        { label: "E&xit", shortcut: "Alt+F4", nativeShortcut: true, onSelect: a.onExit, hidden: !a.desktop },
      ],
    },
    {
      id: "settings",
      label: "&Settings",
      items: [
        { label: "&Preferences…", shortcut: "Ctrl+,", onSelect: a.onPreferences },
        { type: "separator" },
        { label: "&Maximum Compression", checked: s.maxCompression, onSelect: toggle("maxCompression") },
        { label: "Mark Files as &Crunched", checked: s.markAsCrunched, onSelect: toggle("markAsCrunched") },
        { label: "S&kip Crunched Files", checked: s.skipCrunched, onSelect: toggle("skipCrunched") },
        { label: "&Verbose Logging", checked: s.verboseLogging, onSelect: toggle("verboseLogging") },
      ],
    },
    {
      id: "view",
      label: "&View",
      items: [
        { type: "submenu", label: "&Theme", items: themeItems(s.theme, a.onTheme) },
        { type: "separator" },
        { label: "Show &Dev Menu", checked: s.showDevMenu, onSelect: toggle("showDevMenu") },
      ],
    },
    {
      id: "dev",
      label: "&Dev",
      hidden: !s.showDevMenu,
      items: [
        { type: "submenu", label: "&Theme", items: themeItems(s.theme, a.onTheme) },
        { label: "&Next Theme", shortcut: "Ctrl+Shift+T", onSelect: a.onNextTheme },
        { type: "separator" },
        { label: "&Reload UI", shortcut: "Ctrl+R", onSelect: a.onReload, disabled: running },
        { label: "Toggle &DevTools", shortcut: "F12", nativeShortcut: true, onSelect: a.onToggleDevtools, hidden: !a.desktop },
        { type: "separator", hidden: !a.desktop },
        { label: "Open &Settings Folder", onSelect: () => a.onOpen("settings-folder"), hidden: !a.desktop },
        { label: "Open &Log Folder", onSelect: () => a.onOpen("log-folder"), hidden: !a.desktop },
      ],
    },
    {
      id: "help",
      label: "&Help",
      items: [
        { label: "FlacCrunch on &GitHub", onSelect: () => a.onOpen("project"), hidden: !a.desktop },
        { label: "&Release Notes", onSelect: () => a.onOpen("releases"), hidden: !a.desktop },
        { label: "Report an &Issue", onSelect: () => a.onOpen("issues"), hidden: !a.desktop },
        { type: "separator", hidden: !a.desktop },
        { label: "&About FlacCrunch", onSelect: a.onAbout },
      ],
    },
  ];
}
