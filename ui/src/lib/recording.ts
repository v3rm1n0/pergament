import { convertFileSrc } from "@tauri-apps/api/core";
import { api, type Media, type MediaDownload, type MediaFile, type MediaRef } from "@/lib/api";
import type { PlayerItem } from "@/lib/player";
import { t } from "@/lib/i18n";

const sources = (files: MediaFile[]) =>
  files.map((f) => ({ label: f.label, url: f.url, size: f.size, subtitles: f.subtitles }));

/** A catalog recording, streamed from jw.org. */
export const streamItem = (m: Media): PlayerItem => ({
  title: m.title,
  kind: m.kind,
  poster: m.image,
  sources: sources(m.files),
});

/** A downloaded file, played from disk through the asset protocol. */
export const localItem = (d: MediaDownload): PlayerItem => ({
  title: d.title,
  kind: d.kind,
  poster: d.image,
  sources: [
    {
      label: "",
      url: convertFileSrc(d.path),
      size: d.size,
      subtitles: d.subtitlePath ? convertFileSrc(d.subtitlePath) : null,
    },
  ],
});

/** A recording that a publication links to; looks up its files first. */
export async function linkedItem(media: MediaRef): Promise<PlayerItem> {
  const files = await api.mediaLinks(media);
  if (files.length === 0) throw new Error(t("This recording is not available."));
  return { title: files[0].title, kind: media.kind, poster: files[0].poster, sources: sources(files) };
}
