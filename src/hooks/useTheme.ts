import { useEffect } from "react";
import { applyTheme, themeInfo, type ThemeId, type ThemeInfo } from "../themes";

/** Applies the display theme (saved in app settings) as `data-theme` on <html>. */
export function useTheme(theme: ThemeId): ThemeInfo {
  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  return themeInfo(theme);
}
