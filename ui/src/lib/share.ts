const FINDER = "https://www.jw.org/finder";

/** jw.org link of a publication; dated issues name their month. */
export function publicationLink(symbol: string, issueTag: number, langCode: string): string {
  const issue = issueTag > 0 ? `&issue=${String(issueTag).slice(0, 6)}` : "";
  return `${FINDER}?pub=${symbol}${issue}&wtlocale=${langCode}`;
}

/** jw.org link of a recording. Its key carries the language (`pub-nwtsv_X_1_VIDEO`), the link must not. */
export function mediaLink(key: string, langCode: string): string {
  const agnostic = key.replace(`_${langCode}_`, "_");
  return `${FINDER}?lank=${agnostic}&wtlocale=${langCode}`;
}
