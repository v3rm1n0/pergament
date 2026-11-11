// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { StrictMode } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

afterEach(cleanup);

const entry = (local: string | null) => ({ item: { symbol: "lff" }, local });
const removePublication = vi.hoisted(() => vi.fn());
vi.mock("@/app", () => ({
  useApp: () => ({ toast: vi.fn(), favorites: [], toggleFavorite: vi.fn(), push: vi.fn(), download: vi.fn(), removePublication, lang: "X" }),
}));
vi.mock("@/lib/api", () => ({
  api: {
    catalogLanguages: vi.fn(async () => [{ code: "X", entry: entry("lff_2") }, { code: "E", entry: entry(null) }]),
    languages: vi.fn(async () => [{ code: "X", name: "German", vernacular: "Deutsch", rtl: false, signLanguage: false }]),
  },
}));

import { TileMenu } from "@/components/tile-menu";

const target = { kind: "pub", symbol: "lff", issueTag: 0, mepsLanguage: 2, langCode: "X", local: null, title: "Glücklich" } as const;

// Radix opens a menu on a pointer press, not on a click.
const openMenu = () => fireEvent.pointerDown(screen.getByLabelText("More"), { button: 0, ctrlKey: false });

describe("TileMenu", () => {
  it("opens its menu on click", () => {
    render(<StrictMode><div className="group relative"><TileMenu target={target} /></div></StrictMode>);
    openMenu();
    expect(screen.getByText("Share Link")).toBeTruthy();
    expect(screen.getByText("Add to Favorites")).toBeTruthy();
  });

  it("lists languages", async () => {
    render(<StrictMode><div className="group relative"><TileMenu target={target} /></div></StrictMode>);
    openMenu();
    fireEvent.click(screen.getByText("More Languages"));
    await waitFor(() => expect(screen.getByText("Deutsch")).toBeTruthy());
  });

  it("shares", async () => {
    Object.assign(navigator, { clipboard: { writeText: vi.fn(async () => undefined) } });
    render(<StrictMode><div className="group relative"><TileMenu target={target} /></div></StrictMode>);
    openMenu();
    fireEvent.click(screen.getByText("Share Link"));
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith("https://www.jw.org/finder?pub=lff&wtlocale=X"));
  });
});

describe("long press", () => {
  const press = (el: HTMLElement) => fireEvent.pointerDown(el, { pointerType: "touch", clientX: 5, clientY: 5 });

  it("opens the menu from a long press on the tile", () => {
    vi.useFakeTimers();
    try {
      render(<div data-testid="tile" className="group relative"><TileMenu target={target} /></div>);
      press(screen.getByTestId("tile"));
      expect(screen.queryByText("Share Link")).toBeNull();
      act(() => void vi.advanceTimersByTime(500));
      expect(screen.getByText("Share Link")).toBeTruthy();
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not open it from a short press", () => {
    vi.useFakeTimers();
    try {
      render(<div data-testid="tile" className="group relative"><TileMenu target={target} /></div>);
      press(screen.getByTestId("tile"));
      fireEvent.pointerUp(screen.getByTestId("tile"));
      act(() => void vi.advanceTimersByTime(1000));
      expect(screen.queryByText("Share Link")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("Remove", () => {
  it("is only offered for a downloaded publication", () => {
    render(<StrictMode><div className="group relative"><TileMenu target={target} /></div></StrictMode>);
    openMenu();
    expect(screen.queryByText("Remove")).toBeNull();
  });

  it("asks first, then deletes", () => {
    removePublication.mockClear();
    render(
      <StrictMode>
        <div className="group relative"><TileMenu target={{ ...target, local: "lff_2" }} /></div>
      </StrictMode>,
    );
    openMenu();
    fireEvent.click(screen.getByText("Remove"));
    expect(removePublication).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(removePublication).toHaveBeenCalledWith("lff_2", "Glücklich");
  });

  it("does nothing when cancelled", () => {
    removePublication.mockClear();
    render(
      <StrictMode>
        <div className="group relative"><TileMenu target={{ ...target, local: "lff_2" }} /></div>
      </StrictMode>,
    );
    openMenu();
    fireEvent.click(screen.getByText("Remove"));
    fireEvent.click(screen.getByText("Cancel"));
    expect(removePublication).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});
