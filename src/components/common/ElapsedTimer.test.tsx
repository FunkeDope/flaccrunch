import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { ElapsedTimer } from "./ElapsedTimer";

describe("ElapsedTimer", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders '00:00:00' initially with null startTime", () => {
    render(<ElapsedTimer startTime={null} running={false} />);
    expect(screen.getByText("00:00:00")).toBeInTheDocument();
  });

  it("renders '00:00:00' when not running", () => {
    render(<ElapsedTimer startTime={Date.now()} running={false} />);
    expect(screen.getByText("00:00:00")).toBeInTheDocument();
  });

  it("renders '00:00:00' initially when running (before first tick)", () => {
    vi.useFakeTimers();
    const startTime = Date.now();
    render(<ElapsedTimer startTime={startTime} running={true} />);
    // Before any tick the initial state is 0
    expect(screen.getByText("00:00:00")).toBeInTheDocument();
  });

  it("updates elapsed after timer ticks", () => {
    vi.useFakeTimers();
    const startTime = Date.now();
    render(<ElapsedTimer startTime={startTime} running={true} />);

    // Advance 5 seconds
    act(() => {
      vi.advanceTimersByTime(5000);
    });

    expect(screen.getByText("00:00:05")).toBeInTheDocument();
  });

  it("stops updating when running is false", () => {
    vi.useFakeTimers();
    const startTime = Date.now();
    const { rerender } = render(<ElapsedTimer startTime={startTime} running={true} />);

    act(() => { vi.advanceTimersByTime(3000); });
    expect(screen.getByText("00:00:03")).toBeInTheDocument();

    // Stop the timer
    rerender(<ElapsedTimer startTime={startTime} running={false} />);
    act(() => { vi.advanceTimersByTime(5000); });
    // Should remain at 3 seconds
    expect(screen.getByText("00:00:03")).toBeInTheDocument();
  });
});
