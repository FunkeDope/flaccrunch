import { useCallback, useEffect, useState } from "react";
import {
  applyTheme,
  nextTheme,
  readStoredTheme,
  storeTheme,
  themeInfo,
  type ThemeId,
  type ThemeInfo,
} from "../themes";

export interface ThemeState {
  theme: ThemeId;
  info: ThemeInfo;
  setTheme: (id: ThemeId) => void;
  cycleTheme: () => void;
}

/** Display theme: persisted in localStorage, applied as `data-theme` on <html>. */
export function useTheme(): ThemeState {
  const [theme, setThemeState] = useState<ThemeId>(readStoredTheme);

  useEffect(() => {
    applyTheme(theme);
    storeTheme(theme);
  }, [theme]);

  const setTheme = useCallback((id: ThemeId) => setThemeState(id), []);
  const cycleTheme = useCallback(() => setThemeState((current) => nextTheme(current)), []);

  return { theme, info: themeInfo(theme), setTheme, cycleTheme };
}
