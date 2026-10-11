import { useState, useEffect, useCallback, useRef } from "react";
import { getVersion } from "@tauri-apps/api/app";
import * as api from "../lib/tauri";
import type { AppSettings } from "../types/settings";
import type { ProcessingSettings } from "../types/processing";

const defaultSettings: AppSettings = {
  threadCount: null,
  logFolder: null,
  maxRetries: 3,
  verboseLogging: false,
  markAsCrunched: false,
  skipCrunched: false,
  maxCompression: false,
};

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  // Latest settings for merging inside callbacks, so rapid successive updates
  // (e.g. a slider drag) never clobber each other.
  const settingsRef = useRef<AppSettings>(defaultSettings);
  useEffect(() => { settingsRef.current = settings; }, [settings]);
  const [cpuCount, setCpuCount] = useState(1);
  const [defaultLogFolder, setDefaultLogFolder] = useState("");
  const [appVersion, setAppVersion] = useState("");

  useEffect(() => {
    api.getCpuCount().then(setCpuCount).catch(() => {});
    api.getDefaultLogFolder().then(setDefaultLogFolder).catch(() => {});
    api.getSettings().then((s) => {
      setSettings({ ...defaultSettings, ...s });
    }).catch(() => {});
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  const updateSettings = useCallback(async (partial: Partial<AppSettings>) => {
    const updated = { ...settingsRef.current, ...partial };
    settingsRef.current = updated;
    setSettings(updated);
    try {
      await api.saveSettings(updated);
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
  }, []);

  const processingSettings: ProcessingSettings = {
    threadCount: settings.threadCount ?? Math.max(1, cpuCount - 1),
    logFolder: settings.logFolder ?? defaultLogFolder,
    maxRetries: settings.maxRetries,
    verboseLogging: settings.verboseLogging,
    markAsCrunched: settings.markAsCrunched,
    skipCrunched: settings.skipCrunched,
    maxCompression: settings.maxCompression,
  };

  return { settings, cpuCount, defaultLogFolder, appVersion, processingSettings, updateSettings };
}
