import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Check, CloudDownload, Music, Play, Trash2 } from "lucide-react";
import { useApp } from "@/app";
import { api, type Media, type MediaDownload, type Progress } from "@/lib/api";
import { formatDuration, sizeMb } from "@/lib/player";
import { cn } from "@/lib/utils";
import { Menu, menuItem } from "@/components/menu";
import { TileMenu } from "@/components/tile-menu";
import { MediaImg } from "@/components/media-img";
import { Progress as ProgressBar } from "@/components/ui/progress";
import { t } from "@/lib/i18n";

/** Where a recording was listed, which is where the app finds it again to download it. */
export interface MediaSource {
  category: string;
  detailed: boolean;
}

/** Downloaded recordings, the downloads in flight, and the actions on them. */
export function useMediaDownloads() {
  const { toast, lang } = useApp();
  const [list, setList] = useState<MediaDownload[]>([]);
  const [progress, setProgress] = useState<Record<string, Progress>>({});

  const refresh = useCallback(async () => {
    try {
      setList(await api.mediaDownloads());
    } catch (e) {
      toast(String(e));
    }
  }, [toast]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const un = listen<Progress>("progress", (e) => {
      const { task, done, total } = e.payload;
      if (!task.startsWith("media:")) return;
      const key = task.slice("media:".length);
      setProgress((p) => {
        if (total != null && done >= total) {
          const { [key]: _, ...rest } = p;
          return rest;
        }
        return { ...p, [key]: e.payload };
      });
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const download = useCallback(
    async (media: Media, label: string, from: MediaSource) => {
      const size = media.files.find((f) => f.label === label)?.size ?? null;
      setProgress((p) => ({ ...p, [media.key]: { task: media.key, done: 0, total: size } }));
      try {
        await api.downloadMedia(lang, from.category, from.detailed, media.key, label);
        toast(t("Downloaded {title}", { title: media.title }));
        await refresh();
      } catch (e) {
        toast(t("Download failed: {error}", { error: String(e) }));
      } finally {
        setProgress((p) => {
          const { [media.key]: _, ...rest } = p;
          return rest;
        });
      }
    },
    [lang, refresh, toast],
  );

  const remove = useCallback(
    async (item: MediaDownload) => {
      try {
        await api.removeMedia(item.id);
        toast(t("Removed {title}", { title: item.title }));
      } catch (e) {
        toast(t("Remove failed: {error}", { error: String(e) }));
      }
      await refresh();
    },
    [refresh, toast],
  );

  const downloaded = useMemo(
    () => new Map(list.filter((m) => m.langCode === lang).map((m) => [m.key, m])),
    [list, lang],
  );
  return { list, downloaded, progress, download, remove };
}

export type MediaDownloads = ReturnType<typeof useMediaDownloads>;

/** The cloud button of a recording: pick a quality to download, or remove the download. */
export function DownloadButton({
  media,
  from,
  store,
  className,
}: {
  media: Media;
  from: MediaSource;
  store: MediaDownloads;
  className?: string;
}) {
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const close = useCallback(() => setAnchor(null), []);
  const saved = store.downloaded.get(media.key);
  const running = store.progress[media.key];
  const icon = "text-fg";

  if (running) {
    return (
      <div className={cn("w-12", className)} role="progressbar" aria-label={t("Downloading")}>
        <ProgressBar value={running.total ? (running.done / running.total) * 100 : null} />
      </div>
    );
  }
  const options = [...media.files].reverse();
  const size = (bytes: number | null) => (bytes ? sizeMb(bytes) : "?");
  return (
    <div className={className}>
      <button
        className="inline-flex size-[26px] items-center justify-center rounded-md border border-line bg-surface hover:bg-bar focus-visible:outline-2 focus-visible:outline-accent"
        aria-label={saved ? t("Downloaded") : t("Download")}
        aria-haspopup="menu"
        aria-expanded={anchor !== null}
        title={saved ? t("Downloaded") : t("Download")}
        onClick={(e) => {
          const rect = e.currentTarget.getBoundingClientRect();
          setAnchor((a) => (a ? null : rect));
        }}
      >
        {saved ? (
          <Check size={14} strokeWidth={1.8} className={icon} />
        ) : (
          <CloudDownload size={14} strokeWidth={1.6} className={icon} />
        )}
      </button>
      {anchor && (
        <Menu anchor={anchor} onClose={close}>
          {saved ? (
            <button
              role="menuitem"
              className={menuItem}
              onClick={() => {
                close();
                void store.remove(saved);
              }}
            >
              {t("Remove download")}
            </button>
          ) : (
            options.map((f) => (
              <button
                key={f.label}
                role="menuitem"
                className={menuItem}
                onClick={() => {
                  close();
                  void store.download(media, f.label, from);
                }}
              >
                {f.label
                  ? t("Download {label} ({size})", { label: f.label, size: size(f.size) })
                  : t("Download ({size})", { size: size(f.size) })}
              </button>
            ))
          )}
        </Menu>
      )}
    </div>
  );
}

function Thumb({ src, className, square }: { src: string | null; className?: string; square?: boolean }) {
  const [failed, setFailed] = useState(false);
  if (!src || failed) {
    return (
      <div className={cn("flex items-center justify-center rounded-md border border-line bg-tile text-muted", className)}>
        {square ? <Music size={26} strokeWidth={1.2} /> : <Play size={26} strokeWidth={1.2} />}
      </div>
    );
  }
  return (
    <MediaImg
      src={src}
      alt=""
      draggable={false}
      onError={() => setFailed(true)}
      className={cn("rounded-md border border-line object-cover", className)}
    />
  );
}

interface CardProps {
  media: Media;
  from: MediaSource;
  store: MediaDownloads;
  onPlay: () => void;
}

/** A video: wide thumbnail with its length, title below. Click streams it. */
export function MediaCard({ media, from, store, onPlay }: CardProps) {
  const { lang } = useApp();
  return (
    <div className="group w-[176px] shrink-0">
      <div className="relative">
        <button onClick={onPlay} title={media.title} aria-label={media.title} className="block w-full">
          <Thumb src={media.image} className="aspect-video w-full" />
        </button>
        {media.duration != null && (
          <span className="pointer-events-none absolute left-1.5 top-1.5 flex items-center gap-1 rounded border border-line bg-surface px-1 text-xs tabular-nums text-fg">
            <Play size={9} fill="currentColor" /> {formatDuration(media.duration)}
          </span>
        )}
        <DownloadButton media={media} from={from} store={store} className="absolute bottom-1.5 right-1.5" />
        <TileMenu target={{ kind: "media", key: media.key, langCode: lang }} />
      </div>
      <button onClick={onPlay} className="mt-1.5 line-clamp-2 text-left text-[13px] font-medium leading-snug hover:underline">
        {media.title}
      </button>
    </div>
  );
}

/** An audio recording: square cover, title and length in one row. */
export function MediaRow({ media, from, store, onPlay }: CardProps) {
  const { lang } = useApp();
  return (
    <div className="group relative flex items-center gap-3 rounded-md border border-line pr-3 hover:bg-bar">
      <button onClick={onPlay} title={media.title} className="flex min-w-0 flex-1 items-center gap-3 text-left">
        <Thumb square src={media.image} className="h-14 w-14 shrink-0" />
        <span className="min-w-0">
          <span className="line-clamp-2 block text-sm leading-snug">{media.title}</span>
          {media.duration != null && <span className="text-xs text-muted">{formatDuration(media.duration)}</span>}
        </span>
      </button>
      <DownloadButton media={media} from={from} store={store} />
      <TileMenu
        target={{ kind: "media", key: media.key, langCode: lang }}
        className="right-12 top-1/2 -translate-y-1/2"
      />
    </div>
  );
}

/** A downloaded recording with a remove button. Click plays the local file. */
export function DownloadedCard({
  item,
  store,
  onPlay,
}: {
  item: MediaDownload;
  store: MediaDownloads;
  onPlay: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  return (
    <div className="group relative flex items-center gap-3 rounded-md border border-line pr-3">
      <button onClick={onPlay} title={item.title} className="flex min-w-0 flex-1 items-center gap-3 text-left">
        <Thumb square={item.kind === "audio"} src={item.image} className="h-14 w-24 shrink-0" />
        <span className="min-w-0">
          <span className="line-clamp-2 block text-sm leading-snug">{item.title}</span>
          <span className="text-xs text-muted">
            {[item.label, item.duration != null ? formatDuration(item.duration) : "", sizeMb(item.size)]
              .filter(Boolean)
              .join(" · ")}
          </span>
        </span>
      </button>
      {confirming ? (
        <span className="flex gap-1 text-sm">
          <button className="rounded-md bg-brand px-2 py-1 text-brand-fg dark:bg-fg dark:text-surface" onClick={() => void store.remove(item)}>
            {t("Remove")}
          </button>
          <button className="px-2 py-1 hover:bg-bar" onClick={() => setConfirming(false)}>
            {t("Cancel")}
          </button>
        </span>
      ) : (
        <button aria-label={t("Remove download")} title={t("Remove download")} onClick={() => setConfirming(true)}>
          <Trash2 size={17} />
        </button>
      )}
      <TileMenu
        target={{ kind: "media", key: item.key, langCode: item.langCode }}
        className="right-12 top-1/2 -translate-y-1/2"
      />
    </div>
  );
}
