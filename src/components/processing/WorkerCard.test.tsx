import { describe, it, expect } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { WorkerCard } from "./WorkerCard";
import type { WorkerStatus } from "../../types/processing";

const NULL_MD5 = "00000000000000000000000000000000";
const HASH_A = "aabbccddaabbccddaabbccddaabbccdd";
const HASH_B = "11223344112233441122334411223344";

function makeWorker(overrides: Partial<WorkerStatus> = {}): WorkerStatus {
  return {
    id: 0,
    state: "idle",
    file: null,
    percent: 0,
    ratio: "",
    ...overrides,
  };
}

// ─── Rendering smoke tests ────────────────────────────────────────────────────

describe("WorkerCard — rendering", () => {
  it("renders worker number (id + 1)", () => {
    render(<WorkerCard worker={makeWorker({ id: 0 })} />);
    expect(screen.getByText("#1")).toBeInTheDocument();
  });

  it("renders worker #4 for id=3", () => {
    render(<WorkerCard worker={makeWorker({ id: 3 })} />);
    expect(screen.getByText("#4")).toBeInTheDocument();
  });

  it("shows 'idle' in file area when file is null", () => {
    render(<WorkerCard worker={makeWorker({ file: null })} />);
    expect(screen.getByText("idle")).toBeInTheDocument();
  });

  it("shows filename (not full path) when file is set", () => {
    render(<WorkerCard worker={makeWorker({ file: "C:\\Music\\album\\song.flac" })} />);
    expect(screen.getByText("song.flac")).toBeInTheDocument();
  });

  it("renders EMB, PRE, OUT labels", () => {
    render(<WorkerCard worker={makeWorker()} />);
    expect(screen.getByText("EMB")).toBeInTheDocument();
    expect(screen.getByText("PRE")).toBeInTheDocument();
    expect(screen.getByText("OUT")).toBeInTheDocument();
  });

  it("applies active class when state is not idle", () => {
    const { container } = render(<WorkerCard worker={makeWorker({ state: "converting" })} />);
    expect(container.querySelector(".active")).toBeInTheDocument();
  });

  it("applies idle-card class when state is idle", () => {
    const { container } = render(<WorkerCard worker={makeWorker({ state: "idle" })} />);
    expect(container.querySelector(".idle-card")).toBeInTheDocument();
  });
});

// ─── Hash color logic ─────────────────────────────────────────────────────────
// These tests verify the color CSS classes applied to hash value spans.

describe("WorkerCard — hash colors: no hashes present", () => {
  it("shows hash-missing dashes for all three when no hashes", () => {
    const { container } = render(<WorkerCard worker={makeWorker()} />);
    const missing = container.querySelectorAll(".hash-missing");
    expect(missing.length).toBeGreaterThanOrEqual(3);
  });
});

describe("WorkerCard — hash colors: src and out match", () => {
  it("shows hash-match for PRE and OUT when src === out", () => {
    const { container } = render(
      <WorkerCard
        worker={makeWorker({
          state: "idle",
          lastSourceHash: HASH_A,
          lastOutputHash: HASH_A,
        })}
      />
    );
    const matchSpans = container.querySelectorAll(".hash-match");
    // Both PRE and OUT should be hash-match
    expect(matchSpans.length).toBeGreaterThanOrEqual(2);
  });
});

describe("WorkerCard — hash colors: src and out mismatch", () => {
  it("shows hash-mismatch for PRE and OUT when src !== out", () => {
    const { container } = render(
      <WorkerCard
        worker={makeWorker({
          state: "idle",
          lastSourceHash: HASH_A,
          lastOutputHash: HASH_B,
        })}
      />
    );
    const mismatchSpans = container.querySelectorAll(".hash-mismatch");
    expect(mismatchSpans.length).toBeGreaterThanOrEqual(2);
  });
});

describe("WorkerCard — hash colors: embedded MD5 matching PRE", () => {
  it("shows hash-match for EMB when embedded === src", () => {
    const { container } = render(
      <WorkerCard
        worker={makeWorker({
          state: "idle",
          lastSourceHash: HASH_A,
          lastOutputHash: HASH_A,
          lastEmbeddedMd5: HASH_A,
        })}
      />
    );
    const matchSpans = container.querySelectorAll(".hash-match");
    // EMB, PRE, and OUT should all be hash-match
    expect(matchSpans.length).toBeGreaterThanOrEqual(3);
  });
});

describe("WorkerCard — hash colors: null embedded MD5", () => {
  it("shows hash-missing for EMB when embedded is null MD5", () => {
    const { container } = render(
      <WorkerCard
        worker={makeWorker({
          state: "idle",
          lastSourceHash: HASH_A,
          lastOutputHash: HASH_A,
          lastEmbeddedMd5: NULL_MD5,
        })}
      />
    );
    const hashRows = container.querySelectorAll(".hash-row");
    // First row is EMB — should contain hash-missing for null MD5
    const embRow = hashRows[0];
    expect(within(embRow as HTMLElement).getByText("null")).toBeInTheDocument();
  });
});

describe("WorkerCard — abbrev hash display", () => {
  it("shows 'null' for null MD5 constant", () => {
    render(
      <WorkerCard
        worker={makeWorker({
          lastEmbeddedMd5: NULL_MD5,
          lastSourceHash: HASH_A,
          lastOutputHash: HASH_A,
        })}
      />
    );
    expect(screen.getByText("null")).toBeInTheDocument();
  });

  it("shows abbreviated hash for long hashes", () => {
    render(
      <WorkerCard
        worker={makeWorker({
          lastSourceHash: HASH_A,
          lastOutputHash: HASH_A,
        })}
      />
    );
    // HASH_A = "aabbccddaabbccddaabbccddaabbccdd"
    // abbrev: first 8 chars + "…" + last 4 chars = "aabbccdd…ccdd"
    const abbrevs = screen.getAllByText("aabbccdd…ccdd");
    expect(abbrevs.length).toBeGreaterThanOrEqual(1);
  });
});

// ─── Progress display ─────────────────────────────────────────────────────────

describe("WorkerCard — progress", () => {
  it("shows percent during converting when percent > 0", () => {
    render(
      <WorkerCard
        worker={makeWorker({ state: "converting", percent: 42, ratio: "0.433" })}
      />
    );
    expect(screen.getByText("42%")).toBeInTheDocument();
  });

  it("shows ratio in OUT row during converting", () => {
    render(
      <WorkerCard
        worker={makeWorker({ state: "converting", percent: 10, ratio: "0.450" })}
      />
    );
    expect(screen.getByText("≈0.450")).toBeInTheDocument();
  });

  it("shows saved% after completion (idle with lastCompressionPct)", () => {
    render(
      <WorkerCard
        worker={makeWorker({ state: "idle", lastCompressionPct: 12.5 })}
      />
    );
    expect(screen.getByText("12.5%↓")).toBeInTheDocument();
  });
});
