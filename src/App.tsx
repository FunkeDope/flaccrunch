import { useState } from "react";
import { AppShell } from "./components/layout/AppShell";
import { FolderSelector } from "./components/folders/FolderSelector";
import { ProcessingDashboard } from "./components/processing/ProcessingDashboard";
import { SummaryView } from "./components/results/SummaryView";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { LogViewer } from "./components/logs/LogViewer";
import { useProcessing } from "./hooks/useProcessing";
import { useSettings } from "./hooks/useSettings";

type Tab = "folders" | "processing" | "results" | "settings" | "logs";

function App() {
  const [activeTab, setActiveTab] = useState<Tab>("folders");
  const processing = useProcessing();
  const settings = useSettings();

  const renderContent = () => {
    switch (activeTab) {
      case "folders":
        return (
          <FolderSelector
            folders={processing.folders}
            onAddFolder={processing.addFolder}
            onRemoveFolder={processing.removeFolder}
            onStart={() => {
              processing.startRun(settings.processingSettings);
              setActiveTab("processing");
            }}
            canStart={processing.folders.length > 0 && processing.status === "idle"}
          />
        );
      case "processing":
        return (
          <ProcessingDashboard
            status={processing.status}
            counters={processing.counters}
            workers={processing.workers}
            recentEvents={processing.recentEvents}
            topCompression={processing.topCompression}
            onCancel={processing.cancelRun}
          />
        );
      case "results":
        return <SummaryView counters={processing.counters} />;
      case "settings":
        return (
          <SettingsPanel
            settings={settings.settings}
            cpuCount={settings.cpuCount}
            onUpdate={settings.updateSettings}
          />
        );
      case "logs":
        return <LogViewer />;
      default:
        return null;
    }
  };

  return (
    <AppShell activeTab={activeTab} onTabChange={setActiveTab}>
      {renderContent()}
    </AppShell>
  );
}

export default App;
