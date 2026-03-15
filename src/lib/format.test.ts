import { describe, it, expect } from "vitest";
import {
  formatBytes,
  formatElapsed,
  formatPercent,
  getStageColor,
  getStageLabel,
  getStatusColor,
  compressionRowClass,
} from "./format";

// ─── formatBytes ────────────────────────────────────────────────────────────

describe("formatBytes", () => {
  it("returns '0 B' for 0", () => expect(formatBytes(0)).toBe("0 B"));
  it("returns '0 B' for negative", () => expect(formatBytes(-1)).toBe("0 B"));
  it("returns bytes for values under 1024", () => expect(formatBytes(512)).toBe("512 B"));
  it("returns KB for values 1024–1048575", () => {
    expect(formatBytes(1024)).toBe("1.00 KB");
    expect(formatBytes(1536)).toBe("1.50 KB");
  });
  it("returns MB for values 1048576–1073741823", () => {
    expect(formatBytes(1048576)).toBe("1.00 MB");
    expect(formatBytes(1572864)).toBe("1.50 MB");
  });
  it("returns GB for values >= 1073741824", () => {
    expect(formatBytes(1073741824)).toBe("1.00 GB");
    expect(formatBytes(2147483648)).toBe("2.00 GB");
  });
  it("boundary: 1023 is bytes", () => expect(formatBytes(1023)).toBe("1023 B"));
  it("boundary: 1048575 is KB", () => expect(formatBytes(1048575)).toMatch(/KB$/));
});

// ─── formatElapsed ──────────────────────────────────────────────────────────

describe("formatElapsed", () => {
  it("formats 0 seconds as 00:00:00", () => expect(formatElapsed(0)).toBe("00:00:00"));
  it("formats 61 seconds as 00:01:01", () => expect(formatElapsed(61)).toBe("00:01:01"));
  it("formats 3661 seconds as 01:01:01", () => expect(formatElapsed(3661)).toBe("01:01:01"));
  it("formats exactly 1 hour as 01:00:00", () => expect(formatElapsed(3600)).toBe("01:00:00"));
  it("formats 23h 59m 59s correctly", () => expect(formatElapsed(86399)).toBe("23:59:59"));
  it("formats 24h as 1d 00:00:00", () => expect(formatElapsed(86400)).toBe("1d 00:00:00"));
  it("formats 25h as 1d 01:00:00", () => expect(formatElapsed(90000)).toBe("1d 01:00:00"));
  it("formats 2 days as 2d 00:00:00", () => expect(formatElapsed(172800)).toBe("2d 00:00:00"));
  it("pads single-digit minutes and seconds", () => expect(formatElapsed(65)).toBe("00:01:05"));
});

// ─── formatPercent ──────────────────────────────────────────────────────────

describe("formatPercent", () => {
  it("formats 0 as '0.00%'", () => expect(formatPercent(0)).toBe("0.00%"));
  it("formats 100 as '100.00%'", () => expect(formatPercent(100)).toBe("100.00%"));
  it("formats 12.345 as '12.35%'", () => expect(formatPercent(12.345)).toBe("12.35%"));
  it("formats fractional values to 2 decimal places", () => expect(formatPercent(33.3)).toBe("33.30%"));
});

// ─── getStageColor ──────────────────────────────────────────────────────────

describe("getStageColor", () => {
  it("returns stage-converting for 'converting'", () => expect(getStageColor("converting")).toBe("stage-converting"));
  it("returns stage-hashing-source for 'hashing-source'", () => expect(getStageColor("hashing-source")).toBe("stage-hashing-source"));
  it("returns stage-hashing-output for 'hashing-output'", () => expect(getStageColor("hashing-output")).toBe("stage-hashing-output"));
  it("returns stage-artwork for 'artwork'", () => expect(getStageColor("artwork")).toBe("stage-artwork"));
  it("returns stage-finalizing for 'finalizing'", () => expect(getStageColor("finalizing")).toBe("stage-finalizing"));
  it("returns stage-idle for unknown stages", () => {
    expect(getStageColor("idle")).toBe("stage-idle");
    expect(getStageColor("")).toBe("stage-idle");
    expect(getStageColor("unknown")).toBe("stage-idle");
  });
});

// ─── getStageLabel ──────────────────────────────────────────────────────────

describe("getStageLabel", () => {
  it("returns 'Converting' for 'converting'", () => expect(getStageLabel("converting")).toBe("Converting"));
  it("returns 'Hash src' for 'hashing-source'", () => expect(getStageLabel("hashing-source")).toBe("Hash src"));
  it("returns 'Hash out' for 'hashing-output'", () => expect(getStageLabel("hashing-output")).toBe("Hash out"));
  it("returns 'Art opt' for 'artwork'", () => expect(getStageLabel("artwork")).toBe("Art opt"));
  it("returns 'Finalizing' for 'finalizing'", () => expect(getStageLabel("finalizing")).toBe("Finalizing"));
  it("returns 'Idle' for unknown stages", () => {
    expect(getStageLabel("idle")).toBe("Idle");
    expect(getStageLabel("")).toBe("Idle");
  });
});

// ─── getStatusColor ─────────────────────────────────────────────────────────

describe("getStatusColor", () => {
  it("returns 'status-ok' for OK", () => expect(getStatusColor("OK")).toBe("status-ok"));
  it("returns 'status-fail' for FAIL", () => expect(getStatusColor("FAIL")).toBe("status-fail"));
  it("returns 'status-retry' for RETRY", () => expect(getStatusColor("RETRY")).toBe("status-retry"));
  it("returns empty string for unknown status", () => {
    expect(getStatusColor("")).toBe("");
    expect(getStatusColor("other")).toBe("");
  });
});

// ─── compressionRowClass ─────────────────────────────────────────────────────

describe("compressionRowClass", () => {
  it("returns 'comp-excellent' for >= 20%", () => {
    expect(compressionRowClass(20)).toBe("comp-excellent");
    expect(compressionRowClass(50)).toBe("comp-excellent");
  });
  it("returns 'comp-good' for >= 10% and < 20%", () => {
    expect(compressionRowClass(10)).toBe("comp-good");
    expect(compressionRowClass(19.9)).toBe("comp-good");
  });
  it("returns 'comp-fair' for >= 5% and < 10%", () => {
    expect(compressionRowClass(5)).toBe("comp-fair");
    expect(compressionRowClass(9.9)).toBe("comp-fair");
  });
  it("returns 'comp-poor' for > 0% and < 5%", () => {
    expect(compressionRowClass(0.1)).toBe("comp-poor");
    expect(compressionRowClass(4.9)).toBe("comp-poor");
  });
  it("returns 'comp-none' for 0 or negative", () => {
    expect(compressionRowClass(0)).toBe("comp-none");
    expect(compressionRowClass(-1)).toBe("comp-none");
  });
  it("boundary: exactly 5 is 'comp-fair'", () => expect(compressionRowClass(5)).toBe("comp-fair"));
  it("boundary: exactly 10 is 'comp-good'", () => expect(compressionRowClass(10)).toBe("comp-good"));
  it("boundary: exactly 20 is 'comp-excellent'", () => expect(compressionRowClass(20)).toBe("comp-excellent"));
});
