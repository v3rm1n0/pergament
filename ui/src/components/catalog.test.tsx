// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import type { CatalogEntry } from "@/lib/api";

const push = vi.fn();
const download = vi.fn(async () => "lff_2");
vi.mock("@/app", () => ({
  useApp: () => ({ push, download, downloads: {} }),
  taskKey: (e: CatalogEntry) => `download:${e.item.symbol}:${e.item.issue_tag}`,
}));
vi.mock("@/lib/api", () => ({ api: {} }));

import { useEntryAction } from "@/components/catalog";

const entry = (local: string | null) => ({ item: { symbol: "lff", issue_tag: 0 }, local }) as CatalogEntry;

describe("useEntryAction", () => {
  it("opens a downloaded entry", async () => {
    const { result } = renderHook(() => useEntryAction());
    await result.current.activate(entry("lff_2"));
    expect(push).toHaveBeenCalledWith({ name: "publication", dir: "lff_2" });
    expect(download).not.toHaveBeenCalled();
  });

  it("downloads an entry without opening it", async () => {
    push.mockClear();
    const { result } = renderHook(() => useEntryAction());
    await result.current.activate(entry(null));
    expect(download).toHaveBeenCalledTimes(1);
    expect(push).not.toHaveBeenCalled();
  });
});
