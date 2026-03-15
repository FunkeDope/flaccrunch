import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Badge } from "./Badge";

describe("Badge", () => {
  it("renders OK text", () => {
    render(<Badge status="OK" />);
    expect(screen.getByText("OK")).toBeInTheDocument();
  });

  it("renders FAIL text", () => {
    render(<Badge status="FAIL" />);
    expect(screen.getByText("FAIL")).toBeInTheDocument();
  });

  it("renders RETRY text", () => {
    render(<Badge status="RETRY" />);
    expect(screen.getByText("RETRY")).toBeInTheDocument();
  });

  it("applies status-ok class for OK", () => {
    render(<Badge status="OK" />);
    expect(screen.getByText("OK")).toHaveClass("status-ok");
  });

  it("applies status-fail class for FAIL", () => {
    render(<Badge status="FAIL" />);
    expect(screen.getByText("FAIL")).toHaveClass("status-fail");
  });

  it("applies status-retry class for RETRY", () => {
    render(<Badge status="RETRY" />);
    expect(screen.getByText("RETRY")).toHaveClass("status-retry");
  });
});
