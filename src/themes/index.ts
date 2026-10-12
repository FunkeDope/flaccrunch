/**
 * Display themes. The active theme is stored as `data-theme` on <html>; each
 * theme is a CSS file under src/themes that scopes itself to that attribute.
 * Keep the id list in sync with the inline scripts in index.html/preview.html.
 */
export type ThemeId =
  | "win95"
  | "basic"
  | "basic-dark"
  | "basic-wizard"
  | "winamp"
  | "matrix"
  | "flash"
  | "arcade"
  | "casino"
  | "comic"
  | "dos"
  | "gameboy"
  | "limewire";

export interface ThemeInfo {
  id: ThemeId;
  /** Short label shown in the theme menu. */
  name: string;
  /** One-line description shown in Settings and as the menu tooltip. */
  tagline: string;
  /** Workers as a sortable table (plain apps) or themed cards (everything else). */
  workers: "table" | "cards";
}

export const THEMES: readonly ThemeInfo[] = [
  { id: "win95", name: "Windows 95", tagline: "Teal desktop, bevelled buttons, MS Sans Serif.", workers: "cards" },
  { id: "basic", name: "Basic", tagline: "A plain Windows app: grey dialog, group boxes, list views, standard buttons.", workers: "table" },
  { id: "basic-dark", name: "Basic (Dark)", tagline: "The plain Windows app in Windows dark mode.", workers: "table" },
  { id: "basic-wizard", name: "Wizard", tagline: "The plain app laid out as an installer wizard: header band, centred page, bottom button bar.", workers: "table" },
  { id: "winamp", name: "Winamp", tagline: "Winamp 2 classic skin, copied from the real thing.", workers: "cards" },
  { id: "matrix", name: "Matrix", tagline: "A terminal: phosphor green on black, falling code, process list.", workers: "cards" },
  { id: "flash", name: "Flash Game", tagline: "2000s browser-game HUD: thick outlines, candy buttons, level cards.", workers: "cards" },
  { id: "arcade", name: "Arcade", tagline: "Coin-op cabinet: CRT glow, pixel score counters, INSERT COIN.", workers: "cards" },
  { id: "casino", name: "Casino", tagline: "Slot machine: red velvet, gold chrome, chasing marquee lights.", workers: "cards" },
  { id: "comic", name: "Comic Book", tagline: "Halftone dots, thick ink outlines, speech-bubble panels, POW.", workers: "cards" },
  { id: "dos", name: "DOS", tagline: "Norton Commander on a real DOS screen: black, light grey, double-line frames.", workers: "cards" },
  { id: "gameboy", name: "Game Boy", tagline: "Four-shade LCD green in a grey handheld shell.", workers: "cards" },
  { id: "limewire", name: "LimeWire", tagline: "LimeWire 4 on Windows XP: search pane, results table, downloads.", workers: "cards" },
];

export const DEFAULT_THEME: ThemeId = "basic-dark";

const STORAGE_KEY = "flaccrunch.theme";

export function isThemeId(value: unknown): value is ThemeId {
  return typeof value === "string" && THEMES.some((t) => t.id === value);
}

export function themeInfo(id: ThemeId): ThemeInfo {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

export function nextTheme(id: ThemeId): ThemeId {
  const index = THEMES.findIndex((t) => t.id === id);
  return THEMES[(index + 1) % THEMES.length].id;
}

export function readStoredTheme(): ThemeId {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    return isThemeId(stored) ? stored : DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}

export function storeTheme(id: ThemeId): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, id);
  } catch {
    // Storage can be unavailable (private mode, quota); the theme still applies for this session.
  }
}

export function applyTheme(id: ThemeId): void {
  document.documentElement.dataset.theme = id;
}
