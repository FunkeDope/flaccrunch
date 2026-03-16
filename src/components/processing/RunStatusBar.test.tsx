import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { RunStatusBar } from "./RunStatusBar";
import type { RunCounters } from "../../types/processing";

const defaultCounters: RunCounters = {
  totalFiles: 10,
  processed: 0,
  successful: 0,
  failed: 0,
  totalOriginalBytes: 0,
  totalNewBytes: 0,
  totalSavedBytes: 0,
  totalMetadataSaved: 0,
  totalPaddingSaved: 0,
  totalArtworkSaved: 0,
  totalArtworkRawSaved: 0,
  artworkOptimizedFiles: 0,
  artworkOptimizedBlocks: 0,
  warned: 0,
};

function renderBar(overrides: {
  status?: "idle" | "processing" | "cancelling" | "complete";
  counters?: Partial<RunCounters>;
  livePct?: number;
} = {}) {
  const onCancel = vi.fn();
  const onReset = vi.fn();
  const onExport = vi.fn();
  const { rerender, ...rest } = render(
    <RunStatusBar
      status={overrides.status ?? "processing"}
      counters={{ ...defaultCounters, ...overrides.counters }}
      startTime={Date.now()}
      livePct={overrides.livePct}
      onCancel={onCancel}
      onReset={onReset}
      onExport={onExport}
    />
  );
  return { onCancel, onReset, onExport, rerender, ...rest };
}

describe("RunStatusBar — labels", () => {
  it("shows 'Processing' when status is processing", () => {
    renderBar({ status: "processing" });
    expect(screen.getByText("Processing")).toBeInTheDocument();
  });

  it("shows 'Cancelling' when status is cancelling", () => {
    renderBar({ status: "cancelling" });
    expect(screen.getByText("Cancelling")).toBeInTheDocument();
  });

  it("shows 'Complete' when all succeeded", () => {
    renderBar({
      status: "complete",
      counters: { processed: 10, successful: 10, failed: 0 },
    });
    expect(screen.getByText("Complete")).toBeInTheDocument();
  });

  it("shows 'Failed' when all failed", () => {
    renderBar({
      status: "complete",
      counters: { processed: 10, successful: 0, failed: 10 },
    });
    expect(screen.getByText("Failed")).toBeInTheDocument();
  });

  it("shows 'Complete (with errors)' when some failed", () => {
    renderBar({
      status: "complete",
      counters: { processed: 10, successful: 7, failed: 3 },
    });
    expect(screen.getByText("Complete (with errors)")).toBeInTheDocument();
  });
});

describe("RunStatusBar — buttons", () => {
  it("shows Cancel button while processing", () => {
    renderBar({ status: "processing" });
    expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument();
  });

  it("Cancel button is disabled when status is cancelling", () => {
    renderBar({ status: "cancelling" });
    const btn = screen.getByRole("button", { name: /cancelling/i });
    expect(btn).toBeDisabled();
  });

  it("calls onCancel when Cancel button clicked", () => {
    const { onCancel } = renderBar({ status: "processing" });
    fireEvent.click(screen.getByRole("button", { name: /cancel/i }));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("shows Export Log and New Run buttons on completion", () => {
    renderBar({ status: "complete" });
    expect(screen.getByRole("button", { name: /export log/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /new run/i })).toBeInTheDocument();
  });

  it("does not show Cancel button on completion", () => {
    renderBar({ status: "complete" });
    expect(screen.queryByRole("button", { name: /cancel/i })).toBeNull();
  });

  it("calls onReset when New Run clicked", () => {
    const { onReset } = renderBar({ status: "complete" });
    fireEvent.click(screen.getByRole("button", { name: /new run/i }));
    expect(onReset).toHaveBeenCalledOnce();
  });

  it("calls onExport when Export Log clicked", () => {
    const { onExport } = renderBar({ status: "complete" });
    fireEvent.click(screen.getByRole("button", { name: /export log/i }));
    expect(onExport).toHaveBeenCalledOnce();
  });
});

describe("RunStatusBar — progress percentage", () => {
  it("shows 0% when no files processed", () => {
    renderBar({ counters: { processed: 0, totalFiles: 10 } });
    expect(screen.getByText("0%")).toBeInTheDocument();
  });

  it("shows 50% when half processed", () => {
    renderBar({ counters: { processed: 5, totalFiles: 10 } });
    expect(screen.getByText("50%")).toBeInTheDocument();
  });

  it("shows 100% when all processed", () => {
    renderBar({
      status: "complete",
      counters: { processed: 10, totalFiles: 10, successful: 10 },
    });
    expect(screen.getByText("100%")).toBeInTheDocument();
  });
});

describe("RunStatusBar — stats display", () => {
  it("shows success count chip", () => {
    renderBar({ counters: { successful: 5 } });
    expect(screen.getByText("5")).toBeInTheDocument();
  });

  it("does not show failure chip when failed is 0", () => {
    const { container } = renderBar({ counters: { failed: 0 } });
    expect(container.querySelector(".chip-error")).toBeNull();
  });

  it("shows failure chip when failed > 0", () => {
    const { container } = renderBar({ counters: { failed: 2 } });
    expect(container.querySelector(".chip-error")).toBeInTheDocument();
  });
});
