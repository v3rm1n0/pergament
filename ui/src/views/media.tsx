import { useCallback, useEffect, useState } from "react";
import { AppBar, useApp } from "@/app";
import { api, type Media, type MediaCategory, type MediaDownload } from "@/lib/api";
import { MediaImg } from "@/components/media-img";
import {
  DownloadedCard,
  MediaCard,
  MediaRow,
  useMediaDownloads,
  type MediaDownloads,
  type MediaSource,
} from "@/components/media-card";
import { localItem, streamItem } from "@/lib/recording";
import { t } from "@/lib/i18n";

export const VIDEO_ROOT = "VideoOnDemand";
export const AUDIO_ROOT = "Audio";

/** A category in the publication language, with retry when it cannot be loaded. */
function useCategory(key: string, detailed: boolean) {
  const { lang } = useApp();
  const [cat, setCat] = useState<MediaCategory | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    setCat(null);
    setError(null);
    api
      .mediaCategory(lang, key, detailed)
      .then((c) => live && setCat(c))
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [lang, key, detailed, attempt]);

  return { cat, error, retry: useCallback(() => setAttempt((a) => a + 1), []) };
}

function Status({ error, retry }: { error: string | null; retry: () => void }) {
  if (!error) return <p className="py-10 text-center text-muted">{t("Loading…")}</p>;
  return (
    <div className="max-w-xl bg-tile px-5 py-4 text-sm">
      <p className="mb-3" role="alert">
        {t("Cannot load the recordings: {error}", { error })}
      </p>
      <button className="bg-brand px-4 py-2 text-brand-fg hover:brightness-110" onClick={retry}>
        {t("Try again")}
      </button>
    </div>
  );
}

/** Tile of a category, as in the official app: wide picture for video, cover and name for audio. */
function CategoryTile({ cat, kind, onOpen }: { cat: MediaCategory; kind: "video" | "audio"; onOpen: () => void }) {
  if (kind === "audio") {
    return (
      <button onClick={onOpen} className="flex h-[86px] items-center gap-4 bg-tile pr-4 text-left hover:brightness-125">
        {cat.image ? (
          <MediaImg src={cat.image} alt="" className="h-[86px] w-[86px] shrink-0 object-cover" />
        ) : (
          <span className="h-[86px] w-[86px] shrink-0 bg-tile-1" />
        )}
        <span className="line-clamp-3 text-[0.92rem] leading-snug">{cat.name}</span>
      </button>
    );
  }
  return (
    <button onClick={onOpen} className="relative h-[86px] overflow-hidden bg-tile text-left hover:brightness-125">
      {cat.image && <MediaImg src={cat.image} alt="" className="absolute inset-0 h-full w-full object-cover" />}
      <span className="absolute inset-0 bg-gradient-to-r from-black/75 via-black/35 to-transparent" />
      <span className="relative flex h-full items-center px-6 text-[0.95rem] text-white">
        <span className="line-clamp-3 leading-snug">{cat.name}</span>
      </span>
    </button>
  );
}

const TILES = "grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-1";

/** The Video or Audio tab of the library: the top-level categories. */
export function MediaTab({ kind }: { kind: "video" | "audio" }) {
  const { push } = useApp();
  const { cat, error, retry } = useCategory(kind === "video" ? VIDEO_ROOT : AUDIO_ROOT, false);
  if (!cat) return <Status error={error} retry={retry} />;
  return (
    <div className={TILES}>
      {cat.subcategories.map((c) => (
        <CategoryTile key={c.key} cat={c} kind={kind} onOpen={() => push({ name: "media", key: c.key, title: c.name })} />
      ))}
    </div>
  );
}

function Items({
  media,
  from,
  store,
  play,
}: {
  media: Media[];
  from: MediaSource;
  store: MediaDownloads;
  play: (m: Media) => void;
}) {
  if (media.length > 0 && media.every((m) => m.kind === "audio")) {
    return (
      <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-1">
        {media.map((m) => (
          <MediaRow key={m.key} media={m} from={from} store={store} onPlay={() => play(m)} />
        ))}
      </div>
    );
  }
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(176px,1fr))] gap-x-3 gap-y-5">
      {media.map((m) => (
        <MediaCard key={m.key} media={m} from={from} store={store} onPlay={() => play(m)} />
      ))}
    </div>
  );
}

/** One category: sections with their recordings, or the recordings themselves. */
export function MediaCategoryView({ catKey, title }: { catKey: string; title: string }) {
  const { lang, languageName, push, playItem } = useApp();
  const { cat, error, retry } = useCategory(catKey, true);
  const store = useMediaDownloads();
  const from: MediaSource = { category: catKey, detailed: true };
  const play = (m: Media) => playItem(streamItem(m));

  const sections = cat?.subcategories.filter((c) => c.media.length > 0) ?? [];
  const nested = cat?.subcategories.filter((c) => c.media.length === 0 && c.container) ?? [];
  return (
    <>
      <AppBar title={title} subtitle={languageName(lang)} />
      <div className="flex-1 overflow-y-auto px-5 py-5">
        {!cat ? (
          <Status error={error} retry={retry} />
        ) : (
          <>
            {nested.length > 0 && (
              <div className={`${TILES} mb-8`}>
                {nested.map((c) => (
                  <CategoryTile
                    key={c.key}
                    cat={c}
                    kind="video"
                    onOpen={() => push({ name: "media", key: c.key, title: c.name })}
                  />
                ))}
              </div>
            )}
            {cat.media.length > 0 && <Items media={cat.media} from={from} store={store} play={play} />}
            {sections.map((s) => (
              <section key={s.key} className="mb-8">
                <h2 className="mb-3 text-[1.35rem] font-semibold">{s.name}</h2>
                <Items media={s.media} from={from} store={store} play={play} />
              </section>
            ))}
            {cat.media.length === 0 && sections.length === 0 && nested.length === 0 && (
              <p className="text-muted">{t("Nothing here yet.")}</p>
            )}
          </>
        )}
      </div>
    </>
  );
}

/** Recordings saved in the library, for the Downloaded tab. */
export function DownloadedRecordings({ items, store }: { items: MediaDownload[]; store: MediaDownloads }) {
  const { playItem } = useApp();
  if (items.length === 0) return null;
  return (
    <section className="mb-8">
      <h2 className="mb-3 text-[1.35rem] font-semibold">{t("Recordings")}</h2>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-1">
        {items.map((d) => (
          <DownloadedCard key={d.id} item={d} store={store} onPlay={() => playItem(localItem(d))} />
        ))}
      </div>
    </section>
  );
}
