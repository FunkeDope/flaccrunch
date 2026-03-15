import { useState } from "react";
import { AppShell } from "./components/layout/AppShell";
import { FolderSelector } from "./components/folders/FolderSelector";
import { ProcessingDashboard } from "./components/processing/ProcessingDashboard";
import { SettingsModal } from "./components/settings/SettingsPanel";
import { useProcessing } from "./hooks/useProcessing";
import { useSettings } from "./hooks/useSettings";

function App() {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const processing = useProcessing();
  const settings = useSettings();

  return (
    <AppShell onSettingsClick={() => setSettingsOpen(true)}>
      <FolderSelector
        folders={processing.folders}
        onAddFolder={processing.addFolder}
        onRemoveFolder={processing.removeFolder}
        onStart={() => processing.startRun(settings.processingSettings)}
        onCancel={processing.cancelRun}
        onReset={processing.resetRun}
        canStart={processing.folders.length > 0 && processing.status === "idle"}
        status={processing.status}
        error={processing.error}
      />

      {processing.status !== "idle" && (
        <ProcessingDashboard
          status={processing.status}
          counters={processing.counters}
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
