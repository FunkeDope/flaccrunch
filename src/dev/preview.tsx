/**
 * Dev-only UI preview, served by Vite at /preview.html. Renders the real
 * components with fake data so the themes can be checked in a plain browser,
 * without Tauri. Not part of the production build (only index.html is bundled).
 *
 *   /preview.html?theme=winamp&view=running
 *   view = idle | running | complete | settings | log
 */
import React, { useState } from "react";
import ReactDOM from "react-dom/client";
import "../App.css";
import { AppShell } from "../components/layout/AppShell";
import { FolderSelector } from "../components/folders/FolderSelector";
import { ProcessingDashboard } from "../components/processing/ProcessingDashboard";
import { RunStatusBar } from "../components/processing/RunStatusBar";
import { LogViewerModal } from "../components/processing/LogViewerModal";
import { SettingsModal } from "../components/settings/SettingsPanel";
import { ThemeMenu } from "../components/layout/ThemeMenu";
import { useTheme } from "../hooks/useTheme";
import { DEFAULT_THEME, isThemeId, type ThemeId } from "../themes";
import type { AppSettings } from "../types/settings";
import type { FileEvent, RunCounters, WorkerStatus } from "../types/processing";

type View = "idle" | "running" | "complete" | "settings" | "log";

const params = new URLSearchParams(window.location.search);
const initialView = (params.get("view") as View | null) ?? "idle";
const queryTheme = params.get("theme");
const initialTheme: ThemeId = isThemeId(queryTheme) ? queryTheme : DEFAULT_THEME;

const FOLDERS = [
  "D:\\Music\\Boards of Canada\\Music Has the Right to Children",
  "D:\\Music\\Autechre\\Tri Repetae",
  "D:\\Music\\Aphex Twin\\Selected Ambient Works 85-92",
];

const MD5_A = "9e107d9d372bb6826bd81d3542a419d6";
const MD5_B = "e4d909c290d0fb1ca068ffaddf22cbd0";

function workers(running: boolean): WorkerStatus[] {
  const states: WorkerStatus["state"][] = [
    "converting", "hashing-source", "artwork", "converting", "finalizing", "hashing-output", "idle", "converting",
  ];
  return states.map((state, id) => ({
    id,
    state: running ? state : "idle",
    file: running && state !== "idle" ? `${FOLDERS[id % 3]}\\0${id + 1} - Track ${id + 1}.flac` : null,
    percent: running ? [62, 0, 100, 18, 100, 100, 0, 91][id] : 0,
    ratio: "0.612",
    lastSourceHash: id % 2 === 0 ? MD5_A : undefined,
    lastOutputHash: id % 4 === 0 ? MD5_A : undefined,
    lastEmbeddedMd5: id % 3 === 0 ? MD5_A : id % 3 === 1 ? MD5_B : undefined,
    lastCompressionPct: !running || state === "idle" ? [7.4, 1.2, 12.9, 0.3, 5.5, 9.1, 3.3, 0.0][id] : undefined,
  }));
}

function events(count: number): FileEvent[] {
  return Array.from({ length: count }, (_, i) => {
    const fail = i % 11 === 7;
    const pct = fail ? 0 : ((i * 37) % 160) / 10;
    const before = 28_000_000 + i * 913_117;
    const saved = Math.round((before * pct) / 100);
    return {
      time: `12:${String(10 + (i % 50)).padStart(2, "0")}:${String((i * 7) % 60).padStart(2, "0")}`,
      status: fail ? "FAIL" : i % 9 === 4 ? "WARN" : "OK",
      file: `${FOLDERS[i % 3]}\\${String((i % 12) + 1).padStart(2, "0")} - Track ${i + 1}.flac`,
      attempt: "1",
      verification: fail ? "mismatch: MD5" : "match",
      beforeSize: before,
      afterSize: before - saved,
      savedBytes: saved,
      compressionPct: pct,
      detail: fail ? "Decoded MD5 did not match STREAMINFO" : "",
      artworkSavedBytes: i % 4 === 0 ? 180_000 + i * 1000 : 0,
      artworkRawSavedBytes: 0,
      artworkBlocksOptimized: i % 4 === 0 ? 1 : 0,
    };
  });
}

function counters(complete: boolean): RunCounters {
  const total = 148;
  const processed = complete ? total : 61;
  return {
    totalFiles: total,
    processed,
    successful: processed - (complete ? 3 : 1),
    failed: complete ? 3 : 1,
    totalOriginalBytes: processed * 29_500_000,
    totalNewBytes: processed * 27_100_000,
    totalSavedBytes: processed * 2_400_000,
    totalMetadataSaved: processed * 40_000,
    totalPaddingSaved: processed * 8_192,
    totalArtworkSaved: processed * 120_000,
    totalArtworkRawSaved: 0,
    artworkOptimizedFiles: Math.round(processed / 4),
    artworkOptimizedBlocks: Math.round(processed / 4),
    warned: 2,
    skipped: complete ? 4 : 0,
  };
}

function Preview() {
  const [view, setView] = useState<View>(initialView);
  const [folders, setFolders] = useState<string[]>(FOLDERS);
  const [settings, setSettings] = useState<AppSettings>({
    threadCount: 19,
    logFolder: null,
    maxRetries: 3,
    verboseLogging: true,
    markAsCrunched: false,
    skipCrunched: false,
    maxCompression: true,
    theme: initialTheme,
    showThemeQuickSwitch: true,
  });
  const themeInfo = useTheme(settings.theme);
  const setTheme = (id: ThemeId) => setSettings((s) => ({ ...s, theme: id }));

  const running = view === "running";
  const complete = view === "complete";
  const active = running || complete;
  const status = running ? "processing" : complete ? "complete" : "idle";
  const startTime = Date.now() - 754_000;

  return (
    <>
      <AppShell
        onSettingsClick={() => setView("settings")}
        onAddFolder={() => setFolders((f) => [...f, `D:\\Music\\New Folder ${f.length + 1}`])}
        onAddFiles={() => setFolders((f) => [...f, `D:\\Music\\loose-${f.length + 1}.flac`])}
        showQueueActions={!active}
        theme={settings.theme}
      >
        <div className="app-stack">
          {!active && (
            <div className="status-bar app-summary-bar">
              <p className="status-bar-field">Queue: {folders.length}</p>
              <p className="status-bar-field">Threads: {settings.threadCount}</p>
              <p className="status-bar-field">Retries: {settings.maxRetries}</p>
              <p className="status-bar-field">Logging: {settings.verboseLogging ? "On" : "Off"}</p>
            </div>
          )}
          {active && (
            <RunStatusBar
              status={status}
              counters={counters(complete)}
              startTime={startTime}
              endTime={complete ? startTime + 754_000 : null}
              livePct={running ? 43.6 : 100}
              onCancel={() => setView("complete")}
              onReset={() => setView("idle")}
              onExport={() => {}}
              onShowFullLog={() => setView("log")}
            />
          )}
          <FolderSelector
            folders={folders}
            onAddFolder={() => setFolders((f) => [...f, `D:\\Music\\New Folder ${f.length + 1}`])}
            onAddFiles={() => {}}
            onRemoveFolder={(folder) => setFolders((f) => f.filter((x) => x !== folder))}
            onStart={() => setView("running")}
            onTestStorage={() => {}}
            canStart={folders.length > 0 && !active}
            status={status}
            error={null}
          />
          {active && (
            <ProcessingDashboard workers={workers(running)} recentEvents={events(complete ? 148 : 61)} runFolders={FOLDERS} workerLayout={themeInfo.workers} />
          )}
        </div>
        {settings.showThemeQuickSwitch && <ThemeMenu theme={settings.theme} onSelect={setTheme} />}
        {view === "settings" && (
          <SettingsModal
            settings={settings}
            cpuCount={20}
            defaultLogFolder="C:\\Users\\MediaPC\\AppData\\Roaming\\com.flaccrunch.app\\logs"
            appVersion="1.3.2"
            settingsPath="C:\\Users\\MediaPC\\AppData\\Roaming\\com.flaccrunch.app\\settings.json"
            onUpdate={(p) => setSettings((s) => ({ ...s, ...p }))}
            theme={settings.theme}
            onThemeChange={setTheme}
            onClose={() => setView("idle")}
          />
        )}
        {view === "log" && (
          <LogViewerModal
            text={"FlacCrunch v1.3.2 run log\n\n" + events(40).map((e) => `${e.time}  ${e.status.padEnd(5)} ${e.file}`).join("\n")}
            loading={false}
            error={null}
            onSave={() => {}}
            onClose={() => setView("complete")}
          />
        )}
      </AppShell>
    </>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Preview />
  </React.StrictMode>
);
