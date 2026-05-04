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
      <div className="window modal settings-window">
        <div className="title-bar">
          <div className="title-bar-text">Settings</div>
          <div className="title-bar-controls">
            <button aria-label="Close" onClick={onClose} />
          </div>
        </div>
        <div className="window-body modal-body settings-body">
          {appVersion && (
            <div className="settings-version">
              FlacCrunch v{appVersion}
            </div>
          )}
          <fieldset className="settings-section">
            <legend>Performance</legend>
            <ThreadSlider
              value={threadCount}
              max={cpuCount}
              onChange={(v) => onUpdate({ threadCount: v })}
            />
          </fieldset>
          <fieldset className="settings-section">
            <legend>Processing</legend>
            <div className="settings-group">
              <label className="field-row-stacked">
                <span>
                Max Retries Per File
                  <span className="settings-value">
                    {settings.maxRetries}
                  </span>
                </span>
              </label>
              <input
                type="range"
                min={1}
                max={5}
                value={settings.maxRetries}
                onChange={(e) => onUpdate({ maxRetries: parseInt(e.target.value) })}
              />
            </div>
          </fieldset>
          <fieldset className="settings-section">
            <legend>Logging</legend>
            <div className="settings-group">
              <div className="field-row">
                <input
                  id="verbose-logging"
                  type="checkbox"
                  checked={settings.verboseLogging}
                  onChange={() => onUpdate({ verboseLogging: !settings.verboseLogging })}
                />
                <label htmlFor="verbose-logging">Verbose logging</label>
              </div>
              <div className="settings-hint">Write an EFC-format log to disk after each run</div>
            </div>
            {settings.verboseLogging && (
              <div className="settings-group">
                <div className="field-row-stacked">
                  <label htmlFor="log-folder">Log Folder</label>
                  <span className="settings-hint">
                    {settings.logFolder ? "" : `Default: ${defaultLogFolder}`}
                  </span>
                </div>
                <input
                  id="log-folder"
                  type="text"
                  className="settings-input"
                  placeholder={defaultLogFolder}
                  value={settings.logFolder ?? ""}
                  onChange={(e) => onUpdate({ logFolder: e.target.value || null })}
                  spellCheck={false}
                />
              </div>
            )}
          </fieldset>
          <fieldset className="settings-section">
            <legend>Crunched marker</legend>
            <div className="settings-group">
              <div className="field-row">
                <input
                  id="mark-as-crunched"
                  type="checkbox"
                  checked={settings.markAsCrunched}
                  onChange={() => onUpdate({ markAsCrunched: !settings.markAsCrunched })}
                />
                <label htmlFor="mark-as-crunched">Mark files as Crunched</label>
              </div>
              <div className="settings-hint">
                Stamp re-encoded files with a marker tag (FLACCRUNCH_INFO Vorbis comment) so future
                runs can skip them. Existing tags are preserved.
              </div>
            </div>
            <div className="settings-group">
              <div className="field-row">
                <input
                  id="skip-crunched"
                  type="checkbox"
                  checked={settings.skipCrunched}
                  onChange={() => onUpdate({ skipCrunched: !settings.skipCrunched })}
                />
                <label htmlFor="skip-crunched">Skip previously Crunched files</label>
              </div>
              <div className="settings-hint">
                Detect the marker and skip files already processed by FlacCrunch.
              </div>
            </div>
          </fieldset>
          <div className="field-row settings-actions">
            <button className="default" onClick={onClose}>OK</button>
            <button onClick={onClose}>Cancel</button>
          </div>
        </div>
      </div>
    </div>
  );
}
