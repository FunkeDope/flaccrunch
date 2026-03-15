import { ThreadSlider } from "./ThreadSlider";
import type { AppSettings } from "../../types/settings";

interface SettingsModalProps {
  settings: AppSettings;
  cpuCount: number;
  onUpdate: (partial: Partial<AppSettings>) => void;
  onClose: () => void;
}

export function SettingsModal({ settings, cpuCount, onUpdate, onClose }: SettingsModalProps) {
  const threadCount = settings.threadCount ?? Math.max(1, cpuCount - 1);

  return (
    <div className="modal-overlay" onClick={(e) => {
      if (e.target === e.currentTarget) onClose();
    }}>
      <div className="modal">
        <div className="modal-header">
          <h2>Settings</h2>
          <button className="btn-icon modal-close" onClick={onClose} aria-label="Close">
            <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <line x1="4" y1="4" x2="14" y2="14" />
              <line x1="14" y1="4" x2="4" y2="14" />
            </svg>
          </button>
        </div>

        <div className="modal-body">
          <div className="card">
            <h3>Performance</h3>
            <ThreadSlider
              value={threadCount}
              max={cpuCount}
              onChange={(v) => onUpdate({ threadCount: v })}
            />
          </div>

          <div className="card">
            <h3>Processing</h3>
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
            <h3>Log Folder</h3>
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
            <h3>Appearance</h3>
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
      </div>
    </div>
  );
}
