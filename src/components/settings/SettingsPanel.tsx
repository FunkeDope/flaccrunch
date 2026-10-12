import { ThreadSlider } from "./ThreadSlider";
import type { AppSettings } from "../../types/settings";
import { THEMES, isThemeId, type ThemeId } from "../../themes";

interface SettingsModalProps {
  settings: AppSettings;
  cpuCount: number;
  defaultLogFolder: string;
  appVersion?: string;
  settingsPath?: string;
  saveError?: string | null;
  onUpdate: (partial: Partial<AppSettings>) => void;
  /** Active display theme; the picker is hidden when not provided. */
  theme?: ThemeId;
  onThemeChange?: (id: ThemeId) => void;
  onClose: () => void;
}

export function SettingsModal({ settings, cpuCount, defaultLogFolder, appVersion, settingsPath, saveError, onUpdate, theme, onThemeChange, onClose }: SettingsModalProps) {
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
          <div className="settings-scroll">
            {theme && onThemeChange && (
              <fieldset className="settings-section">
                <legend>Display</legend>
                <div className="settings-group">
                  <label className="settings-slider-head" htmlFor="display-theme">
                    <span>Theme</span>
                  </label>
                  <select
                    id="display-theme"
                    className="settings-input"
                    value={theme}
                    onChange={(e) => {
                      if (isThemeId(e.target.value)) onThemeChange(e.target.value);
                    }}
                  >
                    {THEMES.map((t) => (
                      <option key={t.id} value={t.id}>{t.name}</option>
                    ))}
                  </select>
                  <div className="settings-hint">
                    {THEMES.find((t) => t.id === theme)?.tagline}
                  </div>
                </div>
                <div className="settings-group settings-check">
                  <div className="field-row">
                    <input
                      id="show-quick-switch"
                      type="checkbox"
                      checked={settings.showThemeQuickSwitch}
                      onChange={() => onUpdate({ showThemeQuickSwitch: !settings.showThemeQuickSwitch })}
                    />
                    <label htmlFor="show-quick-switch">Show quick switch menu</label>
                  </div>
                  <div className="settings-hint">
                    A small button in the bottom-left corner (or Ctrl+Shift+T) that switches
                    themes from anywhere.
                  </div>
                </div>
              </fieldset>
            )}
            <fieldset className="settings-section">
              <legend>Performance</legend>
              <ThreadSlider
                value={threadCount}
                max={cpuCount}
                onChange={(v) => onUpdate({ threadCount: v })}
              />
              <div className="settings-group">
                <label className="settings-slider-head" htmlFor="max-retries">
                  <span>Max Retries Per File</span>
                  <span className="settings-value">{settings.maxRetries}</span>
                </label>
                <input
                  id="max-retries"
                  className="settings-slider"
                  type="range"
                  min={1}
                  max={5}
                  value={settings.maxRetries}
                  onChange={(e) => onUpdate({ maxRetries: parseInt(e.target.value) })}
                />
                <div className="settings-hint">
                  How many times a file that fails to encode or verify is re-queued before it is
                  reported as failed.
                </div>
              </div>
            </fieldset>

            <fieldset className="settings-section">
              <legend>Processing</legend>
              <div className="settings-group settings-check">
                <div className="field-row">
                  <input
                    id="max-compression"
                    type="checkbox"
                    checked={settings.maxCompression}
                    onChange={() => onUpdate({ maxCompression: !settings.maxCompression })}
                  />
                  <label htmlFor="max-compression">Maximum compression (non-subset)</label>
                </div>
                <div className="settings-hint">
                  Uses prediction order 32 and a denser window search (like flac --lax). Smaller
                  files, slower encode. Still lossless and verified, but some older hardware
                  players cannot play non-subset FLAC.
                </div>
              </div>
            </fieldset>

            <fieldset className="settings-section">
              <legend>Crunched marker</legend>
              <div className="settings-group settings-check">
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
                  Stamp re-encoded files with a marker tag (FLACCRUNCH_INFO Vorbis comment) so
                  future runs can skip them. Existing tags are preserved.
                </div>
              </div>
              <div className="settings-group settings-check">
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

            <fieldset className="settings-section">
              <legend>Logging</legend>
              <div className="settings-group settings-check">
                <div className="field-row">
                  <input
                    id="verbose-logging"
                    type="checkbox"
                    checked={settings.verboseLogging}
                    onChange={() => onUpdate({ verboseLogging: !settings.verboseLogging })}
                  />
                  <label htmlFor="verbose-logging">Verbose logging</label>
                </div>
                <div className="settings-hint">Write an EFC-format log to disk after each run.</div>
              </div>
              {settings.verboseLogging && (
                <div className="settings-group">
                  <label htmlFor="log-folder">Log Folder</label>
                  <input
                    id="log-folder"
                    type="text"
                    className="settings-input"
                    placeholder={defaultLogFolder}
                    value={settings.logFolder ?? ""}
                    onChange={(e) => onUpdate({ logFolder: e.target.value || null })}
                    spellCheck={false}
                  />
                  <div className="settings-hint">Leave blank to use the default folder shown.</div>
                </div>
              )}
            </fieldset>
          </div>

          <div className="settings-footer">
            {saveError ? (
              <div className="error-banner">{saveError}</div>
            ) : (
              settingsPath && (
                <div className="settings-hint settings-path" title={settingsPath}>
                  Saved automatically to {settingsPath}
                </div>
              )
            )}
            <div className="field-row settings-actions">
              {appVersion && (
                <span className="settings-version">FlacCrunch v{appVersion}</span>
              )}
              <button className="default" onClick={onClose}>OK</button>
              <button onClick={onClose}>Cancel</button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
