import React, { useState } from "react";
import ReactDOM from "react-dom/client";
import "./App.css";
import { SettingsModal } from "./components/settings/SettingsPanel";
import type { AppSettings } from "./types/settings";

function Preview() {
  const [settings, setSettings] = useState<AppSettings>({
    threadCount: 19, logFolder: null, maxRetries: 3, verboseLogging: true,
    markAsCrunched: false, skipCrunched: false, maxCompression: true, theme: "basic-dark", showThemeQuickSwitch: false,
  });
  return (
    <SettingsModal
      settings={settings}
      cpuCount={20}
      defaultLogFolder="C:\Users\MediaPC\AppData\Roaming\com.flaccrunch.app\logs"
      appVersion="1.3.2"
      settingsPath="C:\Users\MediaPC\AppData\Roaming\com.flaccrunch.app\settings.json"
      onUpdate={(p) => setSettings((s) => ({ ...s, ...p }))}
      onClose={() => {}}
    />
  );
}
ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><Preview /></React.StrictMode>);
