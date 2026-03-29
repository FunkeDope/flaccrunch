import { getStageColor, getStageLabel } from "../../lib/format";
import type { WorkerStatus } from "../../types/processing";

interface WorkerCardProps {
  worker: WorkerStatus;
}

const NULL_MD5 = "00000000000000000000000000000000";

function isRealHash(h: string | undefined | null): h is string {
  return !!h && h !== NULL_MD5;
}

function hashColors(
  src: string | undefined,
  out: string | undefined,
  emb: string | undefined
): { srcColor: string; outColor: string; embColor: string } {
  const hasSrc = isRealHash(src);
  const hasOut = isRealHash(out);
  const hasEmb = isRealHash(emb);

  const embMatchesSrc = hasEmb && hasSrc && emb === src;

  if (hasOut) {
    let srcColor: string;
    let outColor: string;

    if (hasSrc) {
      if (src === out) {
        srcColor = "hash-match";
        outColor = "hash-match";
      } else {
        srcColor = "hash-mismatch";
        outColor = "hash-mismatch";
      }
    } else {
      srcColor = "hash-missing";
      if (hasEmb) {
        outColor = emb === out ? "hash-match" : "hash-mismatch";
      } else {
        outColor = "hash-warn";
      }
    }

    let embColor: string;
    if (!hasEmb) {
      embColor = "hash-missing";
    } else if (hasSrc) {
      embColor = embMatchesSrc ? "hash-match" : "hash-warn";
    } else {
      embColor = emb === out ? "hash-match" : "hash-warn";
    }

    return { srcColor, outColor, embColor };
  }

  let srcColor: string;
  let embColor: string;

  if (hasSrc && hasEmb) {
    if (embMatchesSrc) {
      srcColor = "hash-match";
      embColor = "hash-match";
    } else {
      srcColor = "hash-warn";
      embColor = "hash-warn";
    }
  } else if (hasSrc) {
    srcColor = "hash-neutral";
    embColor = "hash-missing";
  } else {
    srcColor = "hash-neutral";
    embColor = hasEmb ? "hash-neutral" : "hash-missing";
  }

  return { srcColor, outColor: "hash-neutral", embColor };
}

function abbrev(hash: string | undefined | null): string {
  if (!hash || hash === NULL_MD5) return "null";
  if (hash.length <= 12) return hash;
  return `${hash.slice(0, 8)}…${hash.slice(-4)}`;
}

export function WorkerCard({ worker }: WorkerCardProps) {
  const isActive = worker.state !== "idle";
  const isConverting = worker.state === "converting";
  const isHashingSrc = worker.state === "hashing-source";
  const isHashingOut = worker.state === "hashing-output";

  const embReady = worker.lastEmbeddedMd5 !== undefined;
  const srcReady = !!worker.lastSourceHash;
  const outReady = !!worker.lastOutputHash;

  const { srcColor, outColor, embColor } = hashColors(
    worker.lastSourceHash,
    worker.lastOutputHash,
    worker.lastEmbeddedMd5
  );

  // EMB slot — always rendered
  function embSlot() {
    if (isHashingSrc && !embReady) {
      return <span className="hash-val hash-computing" title="Computing embedded MD5">…</span>;
    }
    if (!embReady) return <span className="hash-val hash-missing" title="No embedded MD5">—</span>;
    const val = worker.lastEmbeddedMd5;
    if (!val || val === NULL_MD5) {
      return <span className="hash-val hash-missing" title="Embedded MD5 is null">null</span>;
    }
    return <span className={`hash-val ${embColor}`} title={val}>{abbrev(val)}</span>;
  }

  // PRE slot — always rendered
  function srcSlot() {
    if (isHashingSrc && !srcReady) {
      return <span className="hash-val hash-computing" title="Computing source hash">…</span>;
    }
    if (!srcReady) return <span className="hash-val hash-missing" title="Source hash unavailable">—</span>;
    return (
      <span className={`hash-val ${srcColor}`} title={worker.lastSourceHash}>
        {abbrev(worker.lastSourceHash)}
      </span>
    );
  }

  // OUT slot — always rendered
  function outSlot() {
    if (isHashingOut && !outReady) {
      return <span className="hash-val hash-computing" title="Computing output hash">…</span>;
    }
    if (!outReady) return <span className="hash-val hash-missing" title="Output hash unavailable">—</span>;
    return (
      <span className={`hash-val ${outColor}`} title={worker.lastOutputHash}>
        {abbrev(worker.lastOutputHash)}
      </span>
    );
  }

  // Ratio shown during encoding; saved% shown after idle with result
  const showRatio = isConverting && !!worker.ratio;
  const showSaved = !isActive && worker.lastCompressionPct !== undefined;

  return (
    <div className={`worker-card worker-panel ${isActive ? "active" : "idle-card"}`}>
      <div className="worker-panel-top">
        <div className="worker-id">#{worker.id + 1}</div>
        <div className="worker-stage-cell">
          <span className={`worker-stage ${getStageColor(worker.state)}`}>
            {getStageLabel(worker.state)}
          </span>
        </div>
      </div>
      <div className="worker-file-cell">
        <div className="file-name" title={worker.file ?? "idle"}>
          {worker.file ? (worker.file.split(/[/\\]/).pop() ?? worker.file) : "idle"}
        </div>

        <div className="worker-progress-row" style={{ visibility: isActive ? "visible" : "hidden" }}>
          <div className="progress-indicator worker-progress">
            <span className="progress-indicator-bar" style={{ width: `${worker.percent}%` }} />
          </div>
          {isConverting && worker.percent > 0 && (
            <span className="worker-percent">{worker.percent}%</span>
          )}
        </div>
      </div>
      <div className="sunken-panel worker-hash-panel">
        <div className="worker-hashes">
          <div className="hash-row">
          <span className="hash-label">EMB</span>
          {embSlot()}
          </div>
          <div className="hash-row">
          <span className="hash-label">PRE</span>
          {srcSlot()}
          </div>
          <div className="hash-row hash-row-out">
            <span className="hash-label">OUT</span>
            {outSlot()}
            {(showRatio || showSaved) && (
              <span className="hash-metric-lane">
                {showRatio && (
                  <span className="hash-extra hash-match" title={`Compression ratio ${worker.ratio}`}>
                    ≈{worker.ratio}
                  </span>
                )}
                {showSaved && (
                  <span
                    className={`hash-extra hash-saved ${worker.lastCompressionPct! >= 5 ? "hash-saved-good" : "hash-saved-low"}`}
                    title={`Saved ${worker.lastCompressionPct!.toFixed(1)}%`}
                  >
                    {worker.lastCompressionPct!.toFixed(1)}%↓
                  </span>
                )}
              </span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
