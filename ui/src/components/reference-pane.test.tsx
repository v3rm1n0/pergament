// @vitest-environment jsdom
// A publication reference on a phone is a sheet, so its header carries the Close button beside Back.
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("@/app", () => ({
  useApp: () => ({ userVersion: 0, download: vi.fn(), downloads: {} }),
  taskKey: () => "",
}));

import { ReferencePane, type PaneRef } from "@/components/reference-pane";

const missing: PaneRef = { kind: "missing", href: "pergament://missing", entry: null, url: null };
const noop = () => undefined;

afterEach(cleanup);

describe("ReferencePane header", () => {
  it("shares one row between Back and Close in a sheet", () => {
    render(
      <ReferencePane pref={missing} onBack={noop} onFollow={noop} onOpenSource={noop} onRetry={noop} onClose={noop} />,
    );
    const back = screen.getByLabelText("Back");
    const close = screen.getByLabelText("Close");
    expect(back.parentElement).toBe(close.parentElement);
  });

  it("has no Close button in a desktop column", () => {
    render(<ReferencePane pref={missing} onBack={noop} onFollow={noop} onOpenSource={noop} onRetry={noop} />);
    expect(screen.queryByLabelText("Close")).toBeNull();
  });
});
