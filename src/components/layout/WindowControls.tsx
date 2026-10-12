import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** The native title bar is off, so the app draws minimise / maximise / close. */
function currentWindow() {
  try {
    return getCurrentWindow();
  } catch {
    return null; // plain-browser preview harness
  }
}

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = currentWindow();
    if (!win) return;
    let unlisten: (() => void) | undefined;
    const refresh = () => win.isMaximized().then(setMaximized).catch(() => {});
    refresh();
    win.onResized(refresh).then((fn) => (unlisten = fn)).catch(() => {});
    return () => unlisten?.();
  }, []);

  return (
    <div className="title-bar-controls app-window-controls">
      <button
        type="button"
        aria-label="Minimize"
        title="Minimize"
        onClick={() => currentWindow()?.minimize().catch(() => {})}
      />
      <button
        type="button"
        aria-label={maximized ? "Restore" : "Maximize"}
        title={maximized ? "Restore Down" : "Maximize"}
        onClick={() => currentWindow()?.toggleMaximize().catch(() => {})}
      />
      <button
        type="button"
        aria-label="Close"
        title="Close"
        onClick={() => currentWindow()?.close().catch(() => {})}
      />
    </div>
  );
}
