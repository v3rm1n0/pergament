// Builders for test data.
import type { CatalogEntry, Media, PubCard } from "@/lib/api";

export const card = (over: Partial<PubCard> = {}): PubCard => ({
  dir: "x_2",
  title: "Title",
  shortTitle: null,
  symbol: "x",
  year: 2026,
  issueTag: "0",
  mepsLanguage: 2,
  langCode: "X",
  publicationType: "Brochure",
  isBible: false,
  cover: null,
  ...over,
});

export const entry = (
  symbol: string,
  category: string | null,
  local: string | null,
  title = symbol,
  issue = 0,
): CatalogEntry => ({
  item: {
    key_symbol: symbol,
    symbol,
    meps_language: 2,
    issue_tag: issue,
    year: 2026,
    title,
    issue_title: null,
    size: 1000,
    sha1: "",
    publication_type: 4,
    short_title: null,
    cataloged_on: null,
    image: null,
    attributes: [],
  },
  category,
  imageUrl: null,
  local,
});

export const media = (key: string, title: string, kind: Media["kind"] = "video"): Media => ({
  key,
  title,
  kind,
  duration: 90,
  published: null,
  image: null,
  files: [
    {
      title,
      label: kind === "video" ? "480p" : "",
      mime: kind === "video" ? "video/mp4" : "audio/mpeg",
      url: "https://example.org/a",
      size: 1e6,
      duration: 90,
      poster: null,
      subtitles: null,
      md5: null,
    },
  ],
});
