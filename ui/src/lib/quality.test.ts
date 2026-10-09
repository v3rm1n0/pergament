import { describe, expect, it } from "vitest";
import { autoStart, stepDown, stepUp, tooManyStalls } from "./quality";

const s = (label: string) => ({ label, url: `u-${label}` });
const all = [s("720p"), s("240p"), s("480p"), s("360p")];

describe("quality", () => {
  it("starts at the largest quality that fits the window", () => {
    expect(autoStart(all, 900)?.label).toBe("720p");
    expect(autoStart(all, 500)?.label).toBe("480p");
    expect(autoStart(all, 400)?.label).toBe("360p");
  });

  it("uses 1080p when the window is tall enough", () => {
    const hd = [...all, s("1080p")];
    expect(autoStart(hd, 1080)?.label).toBe("1080p");
    expect(autoStart(hd, 900)?.label).toBe("720p");
    expect(autoStart(hd, 2160)?.label).toBe("1080p");
  });

  it("does not start below the floor", () => {
    expect(autoStart(all, 200)?.label).toBe("360p");
  });

  it("takes what there is", () => {
    expect(autoStart([s("240p")], 900)?.label).toBe("240p");
    expect(autoStart([s("")], 900)?.label).toBe("");
    expect(autoStart([], 900)).toBeUndefined();
  });

  it("steps down until the smallest", () => {
    expect(stepDown(all, s("720p"))?.label).toBe("480p");
    expect(stepDown(all, s("240p"))).toBeUndefined();
  });

  it("steps up within the ceiling and past blocked qualities only", () => {
    expect(stepUp(all, s("360p"), 720, new Set())?.label).toBe("480p");
    expect(stepUp(all, s("480p"), 480, new Set())).toBeUndefined();
    expect(stepUp(all, s("360p"), 720, new Set(["u-480p"]))).toBeUndefined();
    expect(stepUp(all, s("720p"), 720, new Set())).toBeUndefined();
  });

  it("calls for a smaller quality after two recent stalls", () => {
    const now = 100_000;
    expect(tooManyStalls([now - 1000], now)).toBe(false);
    expect(tooManyStalls([now - 40_000, now - 1000], now)).toBe(false);
    expect(tooManyStalls([now - 20_000, now - 1000], now)).toBe(true);
  });
});
