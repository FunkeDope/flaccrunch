import { useState, useEffect, useCallback } from "react";
import * as api from "../lib/tauri";
import type { AppSettings } from "../types/settings";
import type { ProcessingSettings } from "../types/processing";

const defaultSettings: AppSettings = {
  threadCount: null,
  logFolder: null,
  maxRetries: 3,
  recentFolders: [],
  theme: "system",
};

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [cpuCount, setCpuCount] = useState(1);
  const [defaultLogFolder, setDefaultLogFolder] = useState("");

  useEffect(() => {
    api.getCpuCount().then(setCpuCount).catch(() => {});
    api.getDefaultLogFolder().then(setDefaultLogFolder).catch(() => {});
    api.getSettings().then(setSettings).catch(() => {});
  }, []);

  const updateSettings = useCallback(
    async (partial: Partial<AppSettings>) => {
      const updated = { ...settings, ...partial };
      setSettings(updated);
      try {
        await api.saveSettings(updated);
      } catch (e) {
        console.error("Failed to save settings:", e);
      }
    },
    [settings]
  );

  const processingSettings: ProcessingSettings = {
    threadCount: settings.threadCount ?? Math.max(1, cpuCount - 1),
    logFolder: settings.logFolder ?? defaultLogFolder,
    maxRetries: settings.maxRetries,
  };

  return {
    settings,
    cpuCount,
    processingSettings,
    updateSettings,
  };
}
