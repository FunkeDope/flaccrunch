import { ThreadSlider } from "./ThreadSlider";
import type { AppSettings } from "../../types/settings";

interface SettingsModalProps {
  settings: AppSettings;
  cpuCount: number;
  defaultLogFolder: string;
  appVersion?: string;
  onUpdate: (partial: Partial<AppSettings>) => void;
  onClose: () => void;
}

export function SettingsModal({ settings, cpuCount, defaultLogFolder, appVersion, onUpdate, onClose }: SettingsModalProps) {
  const threadCount = settings.threadCount ?? Math.max(1, cpuCount - 1);

  return (
    <div className="modal-overlay" onClick={(e) => {
      if (e.target === e.currentTarget) onClose();
    }}>
      <div className="modal">
        <div className="modal-header">
          <h2>Settings</h2>
          <button className="btn-icon modal-close" onClick={onClose} aria-label="Close">
            <svg width="16" height="16" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <line x1="4" y1="4" x2="14" y2="14" />
              <line x1="14" y1="4" x2="4" y2="14" />
            </svg>
          </button>
        </div>
        <div className="modal-body">
          {appVersion && (
            <div style={{ textAlign: "center", marginBottom: 12, color: "var(--text-muted)", fontSize: 12 }}>
              FlacCrunch v{appVersion}
            </div>
          )}
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
            <h3>Logging</h3>
            <div className="settings-group">
              <label className="settings-toggle-row">
                <span>
                  Verbose Logging
                  <span className="settings-hint">Write an EFC-format log to disk after each run</span>
                </span>
                <button
                  className={`toggle ${settings.verboseLogging ? "toggle-on" : ""}`}
                  onClick={() => onUpdate({ verboseLogging: !settings.verboseLogging })}
                  aria-pressed={settings.verboseLogging}
                  aria-label="Toggle verbose logging"
                />
              </label>
            </div>
            {settings.verboseLogging && (
              <div className="settings-group" style={{ marginTop: 10 }}>
                <label>
                  Log Folder
                  <span className="settings-hint">{settings.logFolder ? "" : `Default: ${defaultLogFolder}`}</span>
                </label>
                <input
                  type="text"
                  className="settings-input"
                  placeholder={defaultLogFolder}
                  value={settings.logFolder ?? ""}
                  onChange={(e) => onUpdate({ logFolder: e.target.value || null })}
                  spellCheck={false}
                />
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
