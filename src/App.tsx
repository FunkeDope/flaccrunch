import { useState, useMemo } from "react";
import { AppShell } from "./components/layout/AppShell";
import { FolderSelector } from "./components/folders/FolderSelector";
import { ProcessingDashboard } from "./components/processing/ProcessingDashboard";
import { RunStatusBar } from "./components/processing/RunStatusBar";
import { SettingsModal } from "./components/settings/SettingsPanel";
import { useProcessing } from "./hooks/useProcessing";
import { useSettings } from "./hooks/useSettings";

function App() {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const processing = useProcessing();
  const settings = useSettings();

  const isActive = processing.status !== "idle";

  // Live blended progress: completed files + fractional in-flight worker progress
  const livePct = useMemo(() => {
    if (processing.counters.totalFiles === 0) return 0;
    const inFlightWeight = processing.workers
      .filter((w) => w.state !== "idle")
      .reduce((sum, w) => {
        // Post-encoding stages: encoding is done — count as full weight to
        // prevent the bar jumping backward when percent resets to 0 on stage change.
        if (w.state === "hashing-output" || w.state === "artwork" || w.state === "finalizing") {
          return sum + 1.0;
        }
        return sum + w.percent / 100;
      }, 0);
    return Math.min(
      100,
      ((processing.counters.processed + inFlightWeight) / processing.counters.totalFiles) * 100
    );
  }, [processing.counters, processing.workers]);

  return (
    <AppShell onSettingsClick={() => setSettingsOpen(true)}>
      {isActive && (
        <RunStatusBar
          status={processing.status}
          counters={processing.counters}
          startTime={processing.startTime}
          livePct={livePct}
          onCancel={processing.cancelRun}
          onReset={processing.resetRun}
          onExport={processing.exportLog}
        />
      )}

      <FolderSelector
        folders={processing.folders}
        onAddFolder={processing.addFolder}
        onAddFiles={processing.addFiles}
        onRemoveFolder={processing.removeFolder}
        onStart={() => processing.startRun(settings.processingSettings)}
        canStart={processing.folders.length > 0 && processing.status === "idle"}
        status={processing.status}
        error={processing.error}
        isDragOver={processing.isDragOver}
      />

      {isActive && (
        <ProcessingDashboard
          workers={processing.workers}
          recentEvents={processing.recentEvents}
          topCompression={processing.topCompression}
        />
      )}

      {settingsOpen && (
        <SettingsModal
          settings={settings.settings}
          cpuCount={settings.cpuCount}
          onUpdate={settings.updateSettings}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </AppShell>
  );
}

export default App;
