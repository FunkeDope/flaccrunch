import { Header } from "../layout/Header";
import { ThreadSlider } from "./ThreadSlider";
import type { AppSettings } from "../../types/settings";

interface SettingsPanelProps {
  settings: AppSettings;
  cpuCount: number;
  onUpdate: (partial: Partial<AppSettings>) => void;
}

export function SettingsPanel({ settings, cpuCount, onUpdate }: SettingsPanelProps) {
  const threadCount = settings.threadCount ?? Math.max(1, cpuCount - 1);

  return (
    <div>
      <Header title="Settings" />

      <div className="card">
        <h2>Performance</h2>
        <ThreadSlider
          value={threadCount}
          max={cpuCount}
          onChange={(v) => onUpdate({ threadCount: v })}
        />
      </div>

      <div className="card">
        <h2>Processing</h2>
        <div className="settings-group">
          <label>
            Max Retries Per File
            <span className="settings-value">{settings.maxRetries}</span>
          </label>
          <input
            type="range"
            min={1}
            max={5}
            value={settings.maxRetries}
            onChange={(e) => onUpdate({ maxRetries: parseInt(e.target.value) })}
          />
        </div>
      </div>

      <div className="card">
        <h2>Log Folder</h2>
        <div className="settings-group">
          <label>Custom log folder (leave empty for default)</label>
          <input
            type="text"
            value={settings.logFolder ?? ""}
            placeholder="Default: Desktop/EFC-logs"
            onChange={(e) =>
              onUpdate({ logFolder: e.target.value || null })
            }
          />
        </div>
      </div>

      <div className="card">
        <h2>Appearance</h2>
        <div className="settings-group">
          <label>Theme</label>
          <div style={{ display: "flex", gap: 8 }}>
            {(["system", "dark", "light"] as const).map((t) => (
              <button
                key={t}
                className={`btn ${settings.theme === t ? "btn-primary" : "btn-secondary"}`}
                onClick={() => onUpdate({ theme: t })}
              >
                {t.charAt(0).toUpperCase() + t.slice(1)}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
