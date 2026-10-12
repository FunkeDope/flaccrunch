import iconUrl from "../../../src-tauri/icons/128x128.png";
import type { ExternalTarget } from "../../lib/tauri";

interface AboutModalProps {
  appVersion: string;
  libflacVersion: string;
  /** Desktop only: links open in the browser. */
  onOpen?: (target: ExternalTarget) => void;
  onClose: () => void;
}

/** Help → About FlacCrunch. Same window markup as the other dialogs so themes style it. */
export function AboutModal({ appVersion, libflacVersion, onOpen, onClose }: AboutModalProps) {
  return (
    <div className="modal-overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="window modal about-window" role="dialog" aria-label="About FlacCrunch">
        <div className="title-bar">
          <div className="title-bar-text">About FlacCrunch</div>
          <div className="title-bar-controls">
            <button aria-label="Close" onClick={onClose} />
          </div>
        </div>
        <div className="window-body modal-body about-body">
          <div className="about-head">
            <img className="about-icon" src={iconUrl} alt="" width={64} height={64} />
            <div>
              <div className="about-name">FlacCrunch</div>
              <div className="about-version">Version {appVersion || "—"}</div>
              {libflacVersion && <div className="about-detail">libFLAC {libflacVersion}</div>}
            </div>
          </div>
          <p className="about-text">
            Lossless FLAC recompression with verified audio and album-art optimization. A file
            is only replaced after the decoded audio of the new file matches the original.
          </p>
          <div className="field-row settings-actions about-actions">
            {onOpen && (
              <>
                <button onClick={() => onOpen("project")}>GitHub</button>
                <button onClick={() => onOpen("releases")}>Release Notes</button>
              </>
            )}
            <button className="default" onClick={onClose} autoFocus>OK</button>
          </div>
        </div>
      </div>
    </div>
  );
}
