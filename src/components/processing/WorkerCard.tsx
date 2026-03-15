import { getStageColor, getStageLabel } from "../../lib/format";
import type { WorkerStatus } from "../../types/processing";

interface WorkerCardProps {
  worker: WorkerStatus;
}

const NULL_MD5 = "00000000000000000000000000000000";

function isRealHash(h: string | undefined | null): h is string {
  return !!h && h !== NULL_MD5;
}

/**
 * Derive per-row color classes for EMB, PRE, and OUT given what we know so far.
 *
 * Color semantics (matching the PowerShell script):
 *   hash-match    = green  — value verified correct
 *   hash-mismatch = red    — value confirmed wrong (critical)
 *   hash-warn     = yellow — noteworthy discrepancy (original had bad EMB, or no reference)
 *   hash-neutral  = dim white — present but nothing to compare yet
 *   hash-missing  = muted  — absent / null MD5
 *
 * Two separate checks:
 *   1. EMB vs PRE  (was the original file internally consistent?)
 *      Done as soon as both are known — gives live feedback while encoding.
 *   2. PRE vs OUT  (did audio survive re-encoding?)
 *      Only possible once OUT is computed.
 */
function hashColors(
  src: string | undefined,
  out: string | undefined,
  emb: string | undefined
): { srcColor: string; outColor: string; embColor: string } {
  const hasSrc = isRealHash(src);
  const hasOut = isRealHash(out);
  const hasEmb = isRealHash(emb);

  const embMatchesSrc = hasEmb && hasSrc && emb === src;
  const embMismatchesSrc = hasEmb && hasSrc && emb !== src;

  if (hasOut) {
    // ── Phase 2: POST hash available — primary verification is PRE == OUT ──
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
      // No PRE → fall back to EMB == OUT (MATCH|EMB)
      srcColor = "hash-missing";
      if (hasEmb) {
        outColor = emb === out ? "hash-match" : "hash-mismatch";
      } else {
        outColor = "hash-warn"; // no reference at all
      }
    }

    // EMB: was the original file internally consistent? (EMB == PRE)
    let embColor: string;
    if (!hasEmb) {
      embColor = "hash-missing"; // null / not embedded
    } else if (hasSrc) {
      embColor = embMatchesSrc ? "hash-match" : "hash-warn";
    } else {
      // No PRE, compare EMB vs OUT
      embColor = emb === out ? "hash-match" : "hash-warn";
    }

    return { srcColor, outColor, embColor };
  }

  // ── Phase 1: OUT not yet computed — show EMB vs PRE for early feedback ──
  let srcColor: string;
  let embColor: string;

  if (hasSrc && hasEmb) {
    if (embMatchesSrc) {
      // Original file's MD5 matched what we calculated — all good so far
      srcColor = "hash-match";
      embColor = "hash-match";
    } else if (embMismatchesSrc) {
      // Original file had a wrong embedded MD5 — flag it
      srcColor = "hash-warn";
      embColor = "hash-warn";
    } else {
      srcColor = "hash-neutral";
      embColor = "hash-neutral";
    }
  } else if (hasSrc) {
    // PRE known but EMB is null (no embedded MD5 in original)
    srcColor = "hash-neutral";
    embColor = "hash-missing";
  } else {
    srcColor = "hash-neutral";
    embColor = hasEmb ? "hash-neutral" : "hash-missing";
  }

  return { srcColor, outColor: "hash-neutral", embColor };
}

/** Abbreviated hash for display: first8…last4, or special label */
function abbrev(hash: string | undefined | null): string {
  if (!hash || hash === NULL_MD5) return "null";
  if (hash.length <= 12) return hash;
  return `${hash.slice(0, 8)}…${hash.slice(-4)}`;
}

export function WorkerCard({ worker }: WorkerCardProps) {
  const isActive = worker.state !== "idle";
  const showPercent = worker.state === "converting" && worker.percent > 0;

  const isHashingSrc = worker.state === "hashing-source";
  const isHashingOut = worker.state === "hashing-output";

  // embReady: we received the embedded MD5 value (may be null/NULL_MD5 — still "ready")
  const embReady = worker.lastEmbeddedMd5 !== undefined;
  const srcReady = !!worker.lastSourceHash;
  const outReady = !!worker.lastOutputHash;

  const showHashes = isActive || srcReady || outReady;

  const { srcColor, outColor, embColor } = hashColors(
    worker.lastSourceHash,
    worker.lastOutputHash,
    worker.lastEmbeddedMd5
  );

  // Per-slot display element
  function embSlot() {
    if (isHashingSrc && !embReady) return <span className="hash-val hash-computing">…</span>;
    if (!embReady && !srcReady) return null; // nothing to show yet
    const val = worker.lastEmbeddedMd5;
    if (!val || val === NULL_MD5) {
      return <span className="hash-val hash-missing">null</span>;
    }
    return <span className={`hash-val ${embColor}`}>{abbrev(val)}</span>;
  }

  function srcSlot() {
    if (isHashingSrc && !srcReady) return <span className="hash-val hash-computing">…</span>;
    if (!srcReady) return null;
    return <span className={`hash-val ${srcColor}`}>{abbrev(worker.lastSourceHash)}</span>;
  }

  function outSlot() {
    if (isHashingOut && !outReady) return <span className="hash-val hash-computing">…</span>;
    if (!outReady) return null;
    return <span className={`hash-val ${outColor}`}>{abbrev(worker.lastOutputHash)}</span>;
  }

  const embEl = embSlot();
  const srcEl = srcSlot();
  const outEl = outSlot();
  const anyHash = embEl || srcEl || outEl;

  return (
    <div className={`worker-card ${isActive ? "active" : "idle-card"}`}>
      <div className="worker-header">
        <span className="worker-id">#{worker.id + 1}</span>
        <span className={`worker-stage ${getStageColor(worker.state)}`}>
          {getStageLabel(worker.state)}
        </span>
      </div>
      <div className="file-name">
        {worker.file
          ? (worker.file.split(/[/\\]/).pop() ?? worker.file)
          : "idle"}
      </div>
      {isActive && (
        <div className="worker-progress-row">
          <div className="progress-bar">
            <div className="fill" style={{ width: `${worker.percent}%` }} />
          </div>
          {showPercent && (
            <span className="worker-percent">{worker.percent}%</span>
          )}
        </div>
      )}
      {showHashes && anyHash && (
        <div className="worker-hashes">
          {embEl && (
            <div className="hash-row">
              <span className="hash-label">EMB</span>
              {embEl}
            </div>
          )}
          {srcEl && (
            <div className="hash-row">
              <span className="hash-label">PRE</span>
              {srcEl}
              {/* Ratio lives here when OUT is not yet computed */}
              {!outEl && isActive && worker.ratio ? (
                <span style={{ flexShrink: 0, whiteSpace: "nowrap", color: "var(--success)", fontFamily: "var(--font-mono)", fontSize: 10 }}>≈{worker.ratio}</span>
              ) : null}
              {!outEl && !isActive && worker.lastCompressionPct !== undefined ? (
                <span style={{ flexShrink: 0, whiteSpace: "nowrap", color: worker.lastCompressionPct >= 5 ? "var(--success)" : "var(--text-muted)", fontFamily: "var(--font-mono)", fontSize: 10 }}>
                  {worker.lastCompressionPct.toFixed(1)}%↓
                </span>
              ) : null}
            </div>
          )}
          {outEl && (
            <div className="hash-row">
              <span className="hash-label">OUT</span>
              {outEl}
              {isActive && worker.ratio ? (
                <span style={{ flexShrink: 0, whiteSpace: "nowrap", color: "var(--success)", fontFamily: "var(--font-mono)", fontSize: 10 }}>≈{worker.ratio}</span>
              ) : null}
              {!isActive && worker.lastCompressionPct !== undefined ? (
                <span style={{ flexShrink: 0, whiteSpace: "nowrap", color: worker.lastCompressionPct >= 5 ? "var(--success)" : "var(--text-muted)", fontFamily: "var(--font-mono)", fontSize: 10 }}>
                  {worker.lastCompressionPct.toFixed(1)}%↓
                </span>
              ) : null}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
