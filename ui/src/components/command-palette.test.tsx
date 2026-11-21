// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { PubCard } from "@/lib/api";

const push = vi.fn();
const publications: PubCard[] = [
  { dir: "w26_1", title: "The Watchtower", shortTitle: "Watchtower", symbol: "w", keySymbol: "w", year: 2026, issueTag: "1", mepsLanguage: 0, langCode: "X", publicationType: null, isBible: false, cover: null },
  { dir: "lff", title: "Enjoy Life Forever!", shortTitle: "Enjoy Life", symbol: "lff", keySymbol: "lff", year: 2026, issueTag: "", mepsLanguage: 0, langCode: "X", publicationType: null, isBible: false, cover: null },
];

vi.mock("@/app", () => ({
  useApp: () => ({ publications, push, openTarget: vi.fn(), toast: vi.fn(), lang: "X", bibles: {} }),
}));

import { CommandPalette } from "./command-palette";

afterEach(() => {
  cleanup();
  push.mockReset();
});

describe("CommandPalette", () => {
  it("opens with Ctrl+K and Meta+K, but ignores Alt combinations", () => {
    render(<CommandPalette />);

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(screen.getByRole("dialog")).toBeTruthy();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.keyDown(window, { key: "k", altKey: true, ctrlKey: true });
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("closes with Escape", () => {
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders action and publication groups and filters publication titles locally", () => {
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });

    expect(screen.getByText("Actions")).toBeTruthy();
    expect(screen.getByText("Publications")).toBeTruthy();
    expect(screen.getByText("The Watchtower")).toBeTruthy();
    expect(screen.getByText("Enjoy Life Forever!")).toBeTruthy();

    fireEvent.change(screen.getByRole("combobox"), { target: { value: "watchtower" } });
    expect(screen.getByText("The Watchtower")).toBeTruthy();
    expect(screen.queryByText("Enjoy Life Forever!")).toBeNull();
  });

  it("clears the query and results when closed and reopened", () => {
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = screen.getByRole("combobox");

    fireEvent.change(input, { target: { value: "watchtower" } });
    expect(screen.getByText("The Watchtower")).toBeTruthy();
    expect(screen.queryByText("Enjoy Life Forever!")).toBeNull();

    fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect((screen.getByRole("combobox") as HTMLInputElement).value).toBe("");
    expect(screen.getByText("The Watchtower")).toBeTruthy();
    expect(screen.getByText("Enjoy Life Forever!")).toBeTruthy();
  });

  it("opens a publication when its command is selected", () => {
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.click(screen.getByText("The Watchtower"));
    expect(push).toHaveBeenCalledWith({ name: "publication", dir: "w26_1" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
