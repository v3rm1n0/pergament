// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { hydrateMedia } from "./media";

const picture = (width: number) => {
  const root = document.createElement("div");
  root.innerHTML = '<img src="https://example.test/a.jpg">';
  const img = root.querySelector("img")!;
  Object.defineProperty(img, "naturalWidth", { value: width });
  hydrateMedia(root);
  img.dispatchEvent(new Event("load"));
  return img;
};

describe("hydrateMedia", () => {
  it("marks large images as pictures, so they are scaled down, and leaves icons alone", () => {
    expect(picture(800).classList.contains("photo")).toBe(true);
    expect(picture(40).classList.contains("photo")).toBe(false);
  });
});
