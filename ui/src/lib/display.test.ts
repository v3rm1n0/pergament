import { describe, expect, it } from "vitest";
import { needsSeek, remember, type DisplayMessage } from "./display";

const state = { time: 5, paused: false, rate: 1, volume: 1, muted: false, captions: false };
const media: DisplayMessage = { type: "media", url: "u", kind: "video", poster: null, subtitles: null, ...state };

describe("display", () => {
  it("corrects only noticeable drift", () => {
    expect(needsSeek(10, 10.3)).toBe(false);
    expect(needsSeek(10, 11)).toBe(true);
    expect(needsSeek(12, 10)).toBe(true);
  });

  it("keeps the latest content for a window that opens later", () => {
    expect(remember({ type: "idle" }, media)).toBe(media);
    const moved = remember(media, { type: "media-state", ...state, time: 42, paused: true });
    expect(moved).toMatchObject({ type: "media", url: "u", time: 42, paused: true });
  });

  it("ignores playback state when nothing is playing", () => {
    const idle: DisplayMessage = { type: "idle" };
    expect(remember(idle, { type: "media-state", ...state })).toBe(idle);
    const image: DisplayMessage = { type: "image", src: "s", caption: "c" };
    expect(remember(image, { type: "media-state", ...state })).toBe(image);
  });
});
