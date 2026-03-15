import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { ByteDisplay } from "./ByteDisplay";

describe("ByteDisplay", () => {
  it("renders formatted bytes", () => {
    render(<ByteDisplay bytes={512} />);
    expect(screen.getByText("512 B")).toBeInTheDocument();
  });

  it("renders formatted kilobytes", () => {
    render(<ByteDisplay bytes={1024} />);
    expect(screen.getByText("1.00 KB")).toBeInTheDocument();
  });

  it("renders '0 B' for zero", () => {
    render(<ByteDisplay bytes={0} />);
    expect(screen.getByText("0 B")).toBeInTheDocument();
  });

  it("renders megabytes", () => {
    render(<ByteDisplay bytes={1048576} />);
    expect(screen.getByText("1.00 MB")).toBeInTheDocument();
  });
});
