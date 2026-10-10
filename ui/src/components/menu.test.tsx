// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Menu } from "@/components/menu";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const rect = (left: number, top: number, w: number, h: number) =>
  ({ left, top, right: left + w, bottom: top + h, width: w, height: h, x: left, y: top, toJSON: () => ({}) }) as DOMRect;

/** Renders a 200x100 menu in a 1000x600 window and returns where it ended up. */
function place(anchor: DOMRect) {
  cleanup();
  vi.stubGlobal("innerWidth", 1000);
  vi.stubGlobal("innerHeight", 600);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("role") === "menu" ? rect(0, 0, 200, 100) : rect(0, 0, 0, 0);
  });
  render(
    <Menu anchor={anchor} onClose={() => undefined}>
      <button>Item</button>
    </Menu>,
  );
  const style = screen.getByRole("menu").style;
  return { left: parseFloat(style.left), top: parseFloat(style.top) };
}

describe("Menu", () => {
  it("stays inside the window when the anchor is at the left edge", () => {
    expect(place(rect(10, 300, 20, 20)).left).toBe(8);
  });

  it("stays inside the window when the anchor is at the right edge", () => {
    const { left } = place(rect(970, 300, 20, 20));
    expect(left + 200).toBeLessThanOrEqual(1000 - 8);
  });

  it("lines up with the anchor's right edge when there is room", () => {
    expect(place(rect(480, 300, 20, 20)).left).toBe(300);
  });

  it("opens below when there is no room above, and stays above the bottom edge", () => {
    expect(place(rect(480, 20, 20, 20)).top).toBe(44);
    const { top } = place(rect(480, 90, 20, 20));
    expect(top + 100).toBeLessThanOrEqual(600 - 8);
  });

  it("is used with the keyboard: focus moves in, the arrows move between items, and focus goes back on close", () => {
    cleanup();
    const trigger = document.createElement("button");
    document.body.append(trigger);
    trigger.focus();
    const { unmount } = render(
      <Menu anchor={rect(100, 100, 20, 20)} onClose={() => undefined}>
        <button role="menuitem">First</button>
        <button role="menuitem">Second</button>
        <button role="menuitem">Third</button>
      </Menu>,
    );
    const menu = screen.getByRole("menu");
    expect(document.activeElement).toBe(screen.getByText("First"));
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement).toBe(screen.getByText("Second"));
    fireEvent.keyDown(menu, { key: "End" });
    expect(document.activeElement).toBe(screen.getByText("Third"));
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement).toBe(screen.getByText("First"));
    fireEvent.keyDown(menu, { key: "ArrowUp" });
    expect(document.activeElement).toBe(screen.getByText("Third"));
    unmount();
    expect(document.activeElement).toBe(trigger);
    trigger.remove();
  });
});
