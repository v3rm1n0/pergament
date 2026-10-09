import { useEffect, useState } from "react";
import { ExternalLink, X } from "lucide-react";
import { api, type MediaFile, type MediaRef } from "@/lib/api";
import { t } from "@/lib/i18n";

/** Renditions up to this height are preferred as the default; larger ones stream slowly. */
const DEFAULT_LABEL = 480;

const height = (f: MediaFile) => parseInt(f.label, 10) || 0;

function defaultFile(files: MediaFile[]): MediaFile {
  const fits = files.filter((f) => height(f) <= DEFAULT_LABEL);
  return fits[fits.length - 1] ?? files[0];
}

function size(bytes: number | null): string {
  return bytes == null ? "" : `${(bytes / (1 << 20)).toFixed(1)} MB`;
}

/** Streams a video or audio recording from jw.org; nothing is stored. */
export function MediaPlayer({ media, onClose }: { media: MediaRef; onClose: () => void }) {
  const [files, setFiles] = useState<MediaFile[] | null>(null);
  const [current, setCurrent] = useState<MediaFile | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .mediaLinks(media)
      .then((list) => {
        if (!live) return;
        if (list.length === 0) setError(t("This recording is not available."));
        setFiles(list);
        if (list.length > 0) setCurrent(defaultFile(list));
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [media]);

  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);

  const select = (f: MediaFile) => {
    setError(null);
    setCurrent(f);
  };
  const onPlayError = () => setError(t("This system cannot play the recording here. Open it in your browser instead."));

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onMouseDown={onClose}>
      <div
        role="dialog"
        aria-label={current?.title || t("Recording")}
        onMouseDown={(e) => e.stopPropagation()}
        className="flex w-[min(92vw,820px)] flex-col gap-3 bg-surface p-5 shadow-2xl ring-1 ring-line"
      >
        <div className="flex items-center justify-between gap-3">
          <h2 className="truncate text-lg font-semibold">{current?.title || t("Recording")}</h2>
          <button aria-label={t("Close")} onClick={onClose}>
            <X size={20} />
          </button>
        </div>
        {!files && !error && <p className="py-8 text-center text-muted">{t("Loading…")}</p>}
        {current &&
          (media.kind === "video" ? (
            <video
              key={current.url}
              src={current.url}
              poster={current.poster ?? undefined}
              controls
              autoPlay
              onError={onPlayError}
              className="max-h-[70vh] w-full bg-black"
            />
          ) : (
            <audio key={current.url} src={current.url} controls autoPlay onError={onPlayError} className="w-full" />
          ))}
        {error && <p role="alert">{error}</p>}
        <div className="flex flex-wrap items-center gap-2">
          {files && files.length > 1 && (
            <label className="flex items-center gap-2">
              {t("Quality")}
              <select
                value={current?.url ?? ""}
                onChange={(e) => select(files.find((f) => f.url === e.target.value)!)}
                className="bg-bar px-2 py-1"
              >
                {files.map((f) => (
                  <option key={f.url} value={f.url}>
                    {[f.label, size(f.size)].filter(Boolean).join(" · ")}
                  </option>
                ))}
              </select>
            </label>
          )}
          {current && (
            <button
              className="ml-auto flex items-center gap-1 px-2 py-1 hover:bg-bar"
              onClick={() => void api.openExternal(current.url)}
            >
              <ExternalLink size={16} />
              {t("Open in browser")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
