import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Captions, ExternalLink, Repeat, X } from "lucide-react";
import { api, type MediaRef } from "@/lib/api";
import { isMediaUrl, mediaBlobUrl } from "@/lib/media";
import { SPEEDS, pickSource, sizeMb, type PlayerItem, type PlayerSource } from "@/lib/player";
import { loadMediaQuality, saveMediaQuality } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";

/** Blob URL for a `jwmedia:` image; other URLs are used as they are. */
function useImageUrl(src: string | null): string | undefined {
  const [url, setUrl] = useState<string>();
  useEffect(() => {
    if (!src) return setUrl(undefined);
    if (!isMediaUrl(src)) return setUrl(src);
    let live = true;
    mediaBlobUrl(src)
      .then((u) => live && setUrl(u))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [src]);
  return url;
}

function Shell({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onMouseDown={onClose}>
      <div
        role="dialog"
        aria-label={title}
        onMouseDown={(e) => e.stopPropagation()}
        className="flex w-[min(92vw,820px)] flex-col gap-3 bg-surface p-5 shadow-2xl ring-1 ring-line"
      >
        <div className="flex items-center justify-between gap-3">
          <h2 className="truncate text-lg font-semibold">{title}</h2>
          <button aria-label={t("Close")} onClick={onClose}>
            <X size={20} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

const toggle = "flex items-center gap-1 px-2 py-1 hover:bg-bar aria-pressed:bg-bar aria-pressed:text-accent";

/**
 * Plays a recording with the webview's own media controls. The toolbar adds
 * what they lack: quality, speed, repeat and captions.
 */
export function PlayerDialog({ item, lang, onClose }: { item: PlayerItem; lang: string; onClose: () => void }) {
  const [source, setSource] = useState<PlayerSource | undefined>(() => pickSource(item.sources, loadMediaQuality()));
  const [speed, setSpeed] = useState(1);
  const [repeat, setRepeat] = useState(false);
  const [captions, setCaptions] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const media = useRef<HTMLVideoElement & HTMLAudioElement>(null);
  /** Where to continue after switching quality. */
  const resume = useRef(0);
  const poster = useImageUrl(item.poster);

  const applyCaptions = (on: boolean) => {
    const track = media.current?.textTracks[0];
    if (track) track.mode = on ? "showing" : "disabled";
  };
  useEffect(() => applyCaptions(captions), [captions]);

  const onLoaded = () => {
    const el = media.current;
    if (!el) return;
    el.playbackRate = speed;
    if (resume.current > 0) el.currentTime = resume.current;
    resume.current = 0;
    applyCaptions(captions);
  };
  const choose = (s: PlayerSource) => {
    resume.current = media.current?.currentTime ?? 0;
    const height = parseInt(s.label, 10);
    if (height) saveMediaQuality(height);
    setError(null);
    setSource(s);
  };
  const changeSpeed = (v: number) => {
    setSpeed(v);
    if (media.current) media.current.playbackRate = v;
  };
  const onPlayError = () => setError(t("This system cannot play the recording here. Open it in your browser instead."));
  const remote = source?.url.startsWith("https://") ?? false;

  const common = {
    src: source?.url,
    controls: true,
    autoPlay: true,
    loop: repeat,
    onLoadedMetadata: onLoaded,
    onError: onPlayError,
    // The captions track is created per source; `ref` is shared.
    ref: media,
  };
  const track = source?.subtitles ? (
    <track kind="subtitles" src={source.subtitles} srcLang={lang.toLowerCase()} label={t("Subtitles")} />
  ) : null;

  return (
    <Shell title={item.title} onClose={onClose}>
      {source &&
        (item.kind === "video" ? (
          <video key={source.url} {...common} poster={poster} className="max-h-[70vh] w-full bg-black">
            {track}
          </video>
        ) : (
          <>
            {poster && <img src={poster} alt="" className="mx-auto h-40 w-40 object-cover" />}
            <audio key={source.url} {...common} className="w-full">
              {track}
            </audio>
          </>
        ))}
      {error && <p role="alert">{error}</p>}
      <div className="flex flex-wrap items-center gap-2">
        {item.sources.length > 1 && (
          <label className="flex items-center gap-2">
            {t("Quality")}
            <select
              value={source?.url ?? ""}
              onChange={(e) => choose(item.sources.find((s) => s.url === e.target.value)!)}
              className="bg-bar px-2 py-1"
            >
              {item.sources.map((s) => (
                <option key={s.url} value={s.url}>
                  {[s.label, s.size ? sizeMb(s.size) : ""].filter(Boolean).join(" · ")}
                </option>
              ))}
            </select>
          </label>
        )}
        <label className="flex items-center gap-2">
          {t("Speed")}
          <select value={speed} onChange={(e) => changeSpeed(Number(e.target.value))} className="bg-bar px-2 py-1">
            {SPEEDS.map((v) => (
              <option key={v} value={v}>
                {v === 1 ? t("Normal") : `${v}×`}
              </option>
            ))}
          </select>
        </label>
        <button aria-pressed={repeat} className={toggle} onClick={() => setRepeat((r) => !r)}>
          <Repeat size={16} />
          {t("Repeat")}
        </button>
        {source?.subtitles && (
          <button aria-pressed={captions} className={toggle} onClick={() => setCaptions((c) => !c)}>
            <Captions size={16} />
            {t("Subtitles")}
          </button>
        )}
        {remote && source && (
          <button className={cn(toggle, "ml-auto")} onClick={() => void api.openExternal(source.url)}>
            <ExternalLink size={16} />
            {t("Open in browser")}
          </button>
        )}
      </div>
    </Shell>
  );
}

/** Player for a recording that a publication links to; looks up its files first. */
export function MediaPlayer({ media, lang, onClose }: { media: MediaRef; lang: string; onClose: () => void }) {
  const [item, setItem] = useState<PlayerItem | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .mediaLinks(media)
      .then((files) => {
        if (!live) return;
        if (files.length === 0) return setError(t("This recording is not available."));
        setItem({
          title: files[0].title,
          kind: media.kind,
          poster: files[0].poster,
          sources: files.map((f) => ({ label: f.label, url: f.url, size: f.size, subtitles: f.subtitles })),
        });
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [media]);

  if (item) return <PlayerDialog item={item} lang={lang} onClose={onClose} />;
  return (
    <Shell title={t("Recording")} onClose={onClose}>
      {error ? <p role="alert">{error}</p> : <p className="py-8 text-center text-muted">{t("Loading…")}</p>}
    </Shell>
  );
}
