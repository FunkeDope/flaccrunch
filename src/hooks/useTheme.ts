import { useState, useEffect } from "react";
import type { Theme } from "../types/settings";

export function useTheme() {
  const [theme, setTheme] = useState<Theme>("system");

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "light") {
      root.setAttribute("data-theme", "light");
    } else if (theme === "dark") {
      root.removeAttribute("data-theme");
    } else {
      // System preference
      const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
      if (!prefersDark) {
        root.setAttribute("data-theme", "light");
      } else {
        root.removeAttribute("data-theme");
      }
    }
  }, [theme]);

  return { theme, setTheme };
}
