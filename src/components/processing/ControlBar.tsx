import type { RunStatus } from "../../types/processing";

interface ControlBarProps {
  status: RunStatus;
  onCancel: () => void;
}

export function ControlBar({ status, onCancel }: ControlBarProps) {
  return (
    <div className="control-bar">
      {(status === "processing" || status === "cancelling") && (
        <button
          className="btn btn-danger"
          onClick={onCancel}
          disabled={status === "cancelling"}
        >
          {status === "cancelling" ? "Cancelling..." : "Cancel"}
        </button>
      )}
      {status === "complete" && (
        <span style={{ color: "var(--success)", fontWeight: 600 }}>
          Processing Complete
        </span>
      )}
      {status === "idle" && (
        <span style={{ color: "var(--text-muted)" }}>
          Select folders and press Start to begin
        </span>
      )}
    </div>
  );
}
