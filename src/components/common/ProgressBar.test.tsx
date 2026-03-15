import { describe, it, expect } from "vitest";
import { render, container as _container } from "@testing-library/react";
import { ProgressBar } from "./ProgressBar";

describe("ProgressBar", () => {
  it("renders without crashing", () => {
    const { container } = render(<ProgressBar percent={50} />);
    expect(container.firstChild).toBeInTheDocument();
  });

  it("sets fill width to the given percent", () => {
    const { container } = render(<ProgressBar percent={42} />);
    const fill = container.querySelector(".fill") as HTMLElement;
    expect(fill.style.width).toBe("42%");
  });

  it("sets fill width to 0% when percent is 0", () => {
    const { container } = render(<ProgressBar percent={0} />);
    const fill = container.querySelector(".fill") as HTMLElement;
    expect(fill.style.width).toBe("0%");
  });

  it("sets fill width to 100% at full", () => {
    const { container } = render(<ProgressBar percent={100} />);
    const fill = container.querySelector(".fill") as HTMLElement;
    expect(fill.style.width).toBe("100%");
  });

  it("renders the progress-bar wrapper element", () => {
    const { container } = render(<ProgressBar percent={30} />);
    expect(container.querySelector(".progress-bar")).toBeInTheDocument();
  });
});
