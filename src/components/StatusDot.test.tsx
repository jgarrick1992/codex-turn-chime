import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StatusDot } from "./StatusDot";

describe("StatusDot", () => {
  it("uses the exact needs_input status class", () => {
    const { container } = render(<StatusDot kind="needs_input" />);
    expect(container.firstChild).toHaveClass("status-needs_input");
    expect(container.firstChild).not.toHaveClass("status-blocked");
  });

  it("only animates when pulse is explicitly enabled", () => {
    const { container } = render(<StatusDot kind="running" pulse />);
    expect(container.firstChild).toHaveClass("status-pulse");
  });
});
