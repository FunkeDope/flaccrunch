/**
 * Platform facts the UI needs before React renders. `data-platform` on <html>
 * lets CSS adapt (e.g. room for the macOS traffic lights).
 */
import { isAndroid } from "./android";

export const isMac = typeof navigator !== "undefined" && /Macintosh|Mac OS X/i.test(navigator.userAgent);

/** True inside the Tauri app (false in the plain-browser preview harness). */
export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in (window as unknown as Record<string, unknown>);

/** Desktop app with our own title bar: draw minimise / maximise / close. */
export const drawWindowControls = !isAndroid && !isMac;

export function initPlatform(): void {
  if (isMac) document.documentElement.dataset.platform = "mac";
}
