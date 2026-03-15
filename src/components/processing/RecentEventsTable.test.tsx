import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { RecentEventsTable } from "./RecentEventsTable";
import type { FileEvent } from "../../types/processing";

function makeEvent(overrides: Partial<FileEvent> = {}): FileEvent {
  return {
    time: "12:00:00",
    status: "OK",
    file: "/music/song.flac",
    attempt: "1",
    verification: "MATCH",
    beforeSize: 1000,
    afterSize: 800,
    savedBytes: 200,
    compressionPct: 20,
    artworkSavedBytes: 0,
    detail: "",
    sourceHash: null,
    outputHash: null,
    embeddedMd5: null,
    ...overrides,
  };
}

describe("RecentEventsTable — rendering", () => {
  it("renders table headers", () => {
    render(<RecentEventsTable events={[]} />);
    expect(screen.getByText("Time")).toBeInTheDocument();
    expect(screen.getByText("File")).toBeInTheDocument();
    expect(screen.getByText("Audio")).toBeInTheDocument();
    expect(screen.getByText("%")).toBeInTheDocument();
    expect(screen.getByText("Verify")).toBeInTheDocument();
  });

  it("renders an event row with the filename", () => {
    render(<RecentEventsTable events={[makeEvent()]} />);
    expect(screen.getByText("song.flac")).toBeInTheDocument();
  });

  it("renders only the filename, not the full path", () => {
    render(<RecentEventsTable events={[makeEvent({ file: "/deep/path/to/track.flac" })]} />);
    expect(screen.getByText("track.flac")).toBeInTheDocument();
    expect(screen.queryByText("/deep/path/to/track.flac")).toBeNull();
  });

  it("renders verification text", () => {
    render(<RecentEventsTable events={[makeEvent({ verification: "MATCH" })]} />);
    expect(screen.getByText("MATCH")).toBeInTheDocument();
  });

  it("shows '—' for zero audio savings", () => {
    render(<RecentEventsTable events={[makeEvent({ savedBytes: 0 })]} />);
    // At least one dash cell
    const dashes = screen.getAllByText("—");
    expect(dashes.length).toBeGreaterThanOrEqual(1);
  });

  it("shows formatted bytes for positive savings", () => {
    render(<RecentEventsTable events={[makeEvent({ savedBytes: 1024 })]} />);
    expect(screen.getByText("1.00 KB")).toBeInTheDocument();
  });
});

describe("RecentEventsTable — show more / show all", () => {
  const manyEvents = Array.from({ length: 15 }, (_, i) =>
    makeEvent({ file: `/music/track${i}.flac`, time: `12:00:${String(i).padStart(2, "0")}` })
  );

  it("shows at most 10 rows by default (PREVIEW_COUNT)", () => {
    render(<RecentEventsTable events={manyEvents} />);
    // Events are reversed (newest first), so visible rows are track14…track5.
    // track4 and below are beyond the 10-row limit.
    expect(screen.queryByText("track4.flac")).toBeNull();
  });

  it("shows 'Show all' button when more than 10 events", () => {
    render(<RecentEventsTable events={manyEvents} />);
    expect(screen.getByRole("button", { name: /show all/i })).toBeInTheDocument();
  });

  it("expands to all rows when 'Show all' is clicked", () => {
    render(<RecentEventsTable events={manyEvents} />);
    fireEvent.click(screen.getByRole("button", { name: /show all/i }));
    expect(screen.getByText("track10.flac")).toBeInTheDocument();
  });

  it("respects maxRows prop to limit visible rows", () => {
    render(<RecentEventsTable events={manyEvents} maxRows={3} />);
    expect(screen.queryByText("track3.flac")).toBeNull();
    expect(screen.queryByRole("button", { name: /show all/i })).toBeNull();
  });
});

describe("RecentEventsTable — sorting", () => {
  const events = [
    makeEvent({ file: "/music/z_track.flac", compressionPct: 5 }),
    makeEvent({ file: "/music/a_track.flac", compressionPct: 25 }),
  ];

  it("sorts by file name asc when File header clicked", () => {
    render(<RecentEventsTable events={events} />);
    fireEvent.click(screen.getByText("File"));
    const rows = screen.getAllByRole("row");
    // Header row + data rows
    expect(rows[1]).toHaveTextContent("a_track.flac");
    expect(rows[2]).toHaveTextContent("z_track.flac");
  });
});

describe("RecentEventsTable — compression row classes", () => {
  it("applies comp-excellent for compressionPct >= 20", () => {
    const { container } = render(
      <RecentEventsTable events={[makeEvent({ compressionPct: 25 })]} />
    );
    expect(container.querySelector(".comp-excellent")).toBeInTheDocument();
  });

  it("applies comp-none for compressionPct = 0", () => {
    const { container } = render(
      <RecentEventsTable events={[makeEvent({ compressionPct: 0, savedBytes: 0 })]} />
    );
    expect(container.querySelector(".comp-none")).toBeInTheDocument();
  });
});
