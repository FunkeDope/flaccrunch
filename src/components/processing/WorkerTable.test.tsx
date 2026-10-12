import { describe, it, expect } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { WorkerTable } from "./WorkerTable";
import type { WorkerStatus } from "../../types/processing";

const MD5 = "9e107d9d372bb6826bd81d3542a419d6";

function worker(partial: Partial<WorkerStatus> & { id: number }): WorkerStatus {
  return { state: "idle", file: null, percent: 0, ratio: "", ...partial };
}

const WORKERS: WorkerStatus[] = [
  worker({ id: 0, state: "converting", file: "D:\\m\\b.flac", percent: 40, ratio: "0.6" }),
  worker({ id: 1, state: "hashing-source", file: "D:\\m\\a.flac", percent: 0, lastEmbeddedMd5: MD5 }),
  worker({ id: 2, state: "converting", file: "D:\\m\\c.flac", percent: 90, ratio: "0.5" }),
  worker({ id: 3, lastCompressionPct: 7.5 }),
];

function rowIds(): string[] {
  const rows = within(screen.getByRole("table")).getAllByRole("row").slice(1);
  return rows.map((row) => within(row).getAllByRole("cell")[0].textContent ?? "");
}

describe("WorkerTable", () => {
  it("renders one row per worker with the column headers", () => {
    render(<WorkerTable workers={WORKERS} />);
    for (const label of ["#", "Stage", "File", "Progress", "EMB", "PRE", "OUT", "Saved"]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
    expect(rowIds()).toEqual(["#1", "#2", "#3", "#4"]);
    expect(screen.getByText("b.flac")).toBeInTheDocument();
    expect(screen.getByText("90%")).toBeInTheDocument();
  });

  it("sorts by progress descending on first click, ascending on second", () => {
    render(<WorkerTable workers={WORKERS} />);
    fireEvent.click(screen.getByText("Progress"));
    expect(rowIds()).toEqual(["#3", "#1", "#2", "#4"]);
    fireEvent.click(screen.getByText("Progress"));
    expect(rowIds()).toEqual(["#4", "#2", "#1", "#3"]);
  });

  it("sorts by file name with idle slots last", () => {
    render(<WorkerTable workers={WORKERS} />);
    fireEvent.click(screen.getByText("File"));
    expect(rowIds()).toEqual(["#2", "#1", "#3", "#4"]);
  });

  it("shows the saved percentage for a finished idle slot", () => {
    render(<WorkerTable workers={WORKERS} />);
    expect(screen.getByText("7.5%↓")).toBeInTheDocument();
  });
});
