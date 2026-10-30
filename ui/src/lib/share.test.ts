import { describe, expect, it } from "vitest";
import { mediaLink, publicationLink } from "@/lib/share";

describe("share links", () => {
  it("links a dated publication with its month", () => {
    expect(publicationLink("w26", 20260800, "X")).toBe("https://www.jw.org/finder?pub=w26&issue=202608&wtlocale=X");
  });

  it("leaves the issue out of an undated publication", () => {
    expect(publicationLink("T-ftr", 0, "E")).toBe("https://www.jw.org/finder?pub=T-ftr&wtlocale=E");
  });

  it("drops the language from a recording key", () => {
    expect(mediaLink("pub-nwtsv_X_1_VIDEO", "X")).toBe("https://www.jw.org/finder?lank=pub-nwtsv_1_VIDEO&wtlocale=X");
  });

  it("keeps a recording key without the language", () => {
    expect(mediaLink("pub-jwb-098_7_VIDEO", "X")).toBe("https://www.jw.org/finder?lank=pub-jwb-098_7_VIDEO&wtlocale=X");
  });
});
