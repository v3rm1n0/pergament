// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { StrictMode } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

afterEach(cleanup);

const entry = (local: string | null) => ({ item: { symbol: "lff" }, local });
vi.mock("@/app", () => ({
  useApp: () => ({ toast: vi.fn(), favorites: [], toggleFavorite: vi.fn(), push: vi.fn(), download: vi.fn(), lang: "X" }),
}));
vi.mock("@/lib/api", () => ({
  api: {
    catalogLanguages: vi.fn(async () => [{ code: "X", entry: entry("lff_2") }, { code: "E", entry: entry(null) }]),
    languages: vi.fn(async () => [{ code: "X", name: "German", vernacular: "Deutsch", rtl: false, signLanguage: false }]),
  },
}));

import { TileMenu } from "@/components/tile-menu";

const target = { kind: "pub", symbol: "lff", issueTag: 0, mepsLanguage: 2, langCode: "X" } as const;

describe("TileMenu", () => {
  it("opens its menu on click", () => {
    render(<StrictMode><div className="group relative"><TileMenu target={target} /></div></StrictMode>);
    fireEvent.click(screen.getByLabelText("More"));
    expect(screen.getByText("Share Link")).toBeTruthy();
    expect(screen.getByText("Add to Favorites")).toBeTruthy();
  });

  it("lists languages", async () => {
    render(<StrictMode><div className="group relative"><TileMenu target={target} /></div></StrictMode>);
    fireEvent.click(screen.getByLabelText("More"));
    fireEvent.click(screen.getByText("More Languages"));
    await waitFor(() => expect(screen.getByText("Deutsch")).toBeTruthy());
  });

  it("shares", async () => {
    Object.assign(navigator, { clipboard: { writeText: vi.fn(async () => undefined) } });
    render(<StrictMode><div className="group relative"><TileMenu target={target} /></div></StrictMode>);
    fireEvent.click(screen.getByLabelText("More"));
    fireEvent.click(screen.getByText("Share Link"));
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith("https://www.jw.org/finder?pub=lff&wtlocale=X"));
  });
});
