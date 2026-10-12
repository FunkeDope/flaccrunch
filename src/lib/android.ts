/**
 * Android integration. MainActivity (src-tauri/android-patches/MainActivity.kt)
 * keeps the WebView out of the status bar, camera cutout and gesture bar, and
 * exposes `window.FlacCrunchAndroid` so the page can
 *   - colour the strips behind the system bars to match the active theme, and
 *   - read the Material You (wallpaper) palette on Android 12+.
 */

interface AndroidBridge {
  setSystemBarColors(top: string, bottom: string): void;
  getDynamicColors(): string;
}

declare global {
  interface Window {
    FlacCrunchAndroid?: AndroidBridge;
  }
}

export const isAndroid = typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);

/** System palette tone names (system_accent1_<n>) → Material 3 tone. */
const SYSTEM_TONES: Record<string, number> = {
  "0": 100, "10": 99, "50": 95, "100": 90, "200": 80, "300": 70, "400": 60,
  "500": 50, "600": 40, "700": 30, "800": 20, "900": 10, "1000": 0,
};

const PALETTES: Record<string, string> = {
  accent1: "a1", accent2: "a2", accent3: "a3", neutral1: "n1", neutral2: "n2",
};

/** Publishes the wallpaper palette as --you-<palette>-<tone> on <html>. */
function applyDynamicColors(bridge: AndroidBridge): void {
  let palettes: Record<string, Record<string, string>> | null = null;
  try {
    palettes = JSON.parse(bridge.getDynamicColors() || "null");
  } catch {
    return;
  }
  if (!palettes) return;
  const style = document.documentElement.style;
  for (const [name, short] of Object.entries(PALETTES)) {
    const tones = palettes[name];
    if (!tones) continue;
    for (const [systemTone, hex] of Object.entries(tones)) {
      const tone = SYSTEM_TONES[systemTone];
      if (tone !== undefined) style.setProperty(`--you-${short}-${tone}`, hex);
    }
  }
}

type Rgba = [number, number, number, number];

let probe: CanvasRenderingContext2D | null = null;

/** Any CSS colour (rgb, oklch, color-mix…) → RGBA, via a 1×1 canvas. */
function toRgba(color: string): Rgba {
  if (!probe) {
    const canvas = document.createElement("canvas");
    canvas.width = 1;
    canvas.height = 1;
    probe = canvas.getContext("2d", { willReadFrequently: true });
  }
  if (!probe) return [0, 0, 0, 0];
  probe.clearRect(0, 0, 1, 1);
  probe.fillStyle = "rgba(0, 0, 0, 0)";
  probe.fillStyle = color;
  probe.fillRect(0, 0, 1, 1);
  const d = probe.getImageData(0, 0, 1, 1).data;
  return [d[0], d[1], d[2], d[3] / 255];
}

/** The painted background colour at the horizontal centre of row `y`. */
function backgroundAt(y: number): string {
  const layers: Rgba[] = [];
  for (let el = document.elementFromPoint(window.innerWidth / 2, y); el; el = el.parentElement) {
    const color = toRgba(getComputedStyle(el).backgroundColor);
    if (color[3] === 0) continue;
    layers.push(color);
    if (color[3] >= 0.99) break;
  }
  let rgb: [number, number, number] = [255, 255, 255];
  const bottom = layers[layers.length - 1];
  if (bottom && bottom[3] >= 0.99) {
    rgb = [bottom[0], bottom[1], bottom[2]];
    layers.pop();
  }
  for (const [r, g, b, a] of layers.reverse()) {
    rgb = [rgb[0] * (1 - a) + r * a, rgb[1] * (1 - a) + g * a, rgb[2] * (1 - a) + b * a];
  }
  return `#${rgb.map((v) => Math.round(v).toString(16).padStart(2, "0")).join("")}`;
}

/** Keeps the system bar strips the same colour as the page's top and bottom edges. */
function watchSystemBarColors(bridge: AndroidBridge): void {
  let last = "";
  let timer = 0;

  const sync = () => {
    timer = 0;
    const top = backgroundAt(0);
    const bottom = backgroundAt(window.innerHeight - 1);
    const key = `${top}${bottom}`;
    if (key === last) return;
    last = key;
    bridge.setSystemBarColors(top, bottom);
  };

  const schedule = () => {
    if (!timer) timer = window.setTimeout(() => window.requestAnimationFrame(sync), 80);
  };

  new MutationObserver(schedule).observe(document.documentElement, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["class", "data-theme"],
  });
  window.addEventListener("resize", schedule);
  window.matchMedia?.("(prefers-color-scheme: dark)").addEventListener?.("change", schedule);
  schedule();
}

/** Call once at startup, before the first render. No-op off Android. */
export function initAndroid(): void {
  if (!isAndroid) return;
  document.documentElement.dataset.platform = "android";
  const bridge = window.FlacCrunchAndroid;
  if (!bridge) return;
  applyDynamicColors(bridge);
  watchSystemBarColors(bridge);
}
