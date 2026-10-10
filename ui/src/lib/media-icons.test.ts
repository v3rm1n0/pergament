import { Film, Headphones, Music, Baby, Clapperboard } from "lucide-react";
import { describe, expect, it } from "vitest";
import { categoryIcon } from "./media-icons";

describe("categoryIcon", () => {
  it("picks an icon from the key or the name", () => {
    expect(categoryIcon("AudioOriginalSongs", "Original Songs", "audio")).toBe(Music);
    expect(categoryIcon("VODChildren", "", "video")).toBe(Baby);
    expect(categoryIcon("x", "Movies", "video")).toBe(Clapperboard);
  });

  it("falls back to a generic icon for the kind", () => {
    expect(categoryIcon("Zzz", "Qqq", "video")).toBe(Film);
    expect(categoryIcon("Zzz", "Qqq", "audio")).toBe(Headphones);
  });
});
