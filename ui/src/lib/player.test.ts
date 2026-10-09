import { describe, expect, it } from "vitest";
import { formatDuration, pickSource, sizeMb } from "./player";

const src = (label: string) => ({ label });

describe("player", () => {
  it("picks the best source up to the preferred height", () => {
    const all = [src("720p"), src("240p"), src("480p"), src("360p")];
    expect(pickSource(all, 480)?.label).toBe("480p");
    expect(pickSource(all, 300)?.label).toBe("240p");
    expect(pickSource(all, 2160)?.label).toBe("720p");
  });

  it("falls back to the smallest when all are larger", () => {
    expect(pickSource([src("720p"), src("480p")], 240)?.label).toBe("480p");
  });

  it("takes the only source, e.g. audio without a label", () => {
    expect(pickSource([src("")], 480)?.label).toBe("");
    expect(pickSource([], 480)).toBeUndefined();
  });

  it("formats durations", () => {
    expect(formatDuration(5)).toBe("0:05");
    expect(formatDuration(327.7)).toBe("5:28");
    expect(formatDuration(3723)).toBe("1:02:03");
  });

  it("formats sizes", () => {
    expect(sizeMb(59_000_000)).toBe("59 MB");
    expect(sizeMb(100)).toBe("1 MB");
  });
});
