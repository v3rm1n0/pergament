import { MediaImg } from "@/components/media-img";
import { useEffect, useState } from "react";
import { BookOpen, CalendarDays, ChevronRight, Star } from "lucide-react";
import { AppBar, SectionTitle, useApp } from "@/app";
import { api, type CatalogEntry, type DatedPage, type HomeLists, type PubCard } from "@/lib/api";
import { isoDate, longDate } from "@/lib/dates";
import { themeScripture } from "@/lib/page";
import { inLanguage } from "@/lib/settings";
import { t } from "@/lib/i18n";
import { TileMenu, cardTarget } from "@/components/tile-menu";
import { CatalogPrompt, CoverCaption, CoverTile, EntryCard, useEntryAction } from "@/components/catalog";

/** First row of the Teaching Toolbox: brochures, books, then the current magazines. Everything else, such as tracts, follows on the next row. */
const TOOLBOX_ROW = ["Brochures and Booklets", "Books", "Watchtower", "Awake!"];

/** Publications the Teaching Toolbox leaves out (bh, bhs, lvs, lv, fg and jl). */
const TOOLBOX_HIDDEN = ["bh", "bhs", "lvs", "lv", "fg", "jl"];

/** Splits entries into the two toolbox rows of category groups, keeping the list order within each category. */
function toolboxRows(entries: CatalogEntry[]): CatalogEntry[][][] {
  const shown = entries.filter((e) => !TOOLBOX_HIDDEN.includes(e.item.symbol));
  const group = (categories: (string | null)[]) =>
    categories.map((c) => shown.filter((e) => e.category === c)).filter((g) => g.length > 0);
  const others = [...new Set(shown.map((e) => e.category))].filter((c) => !TOOLBOX_ROW.includes(c ?? ""));
  return [group(TOOLBOX_ROW), group(others)].filter((row) => row.length > 0);
}

function FavoriteTile({ pub }: { pub: PubCard }) {
  const { push, lang } = useApp();
  return (
    <div className="group relative w-[88px]">
      <button
        onClick={() => push({ name: "publication", dir: pub.dir })}
        className="w-full text-left"
        title={pub.title}
      >
        {pub.cover ? (
          <MediaImg src={pub.cover} alt="" className="h-[88px] w-[88px] object-cover" draggable={false} />
        ) : (
          <div className="flex h-[88px] w-[88px] items-center justify-center bg-tile">
            <BookOpen size={28} strokeWidth={1.2} />
          </div>
        )}
        <CoverCaption>{pub.shortTitle ?? pub.title}</CoverCaption>
      </button>
      <TileMenu target={cardTarget(pub, lang)} />
    </div>
  );
}

export function HomeView() {
  const { lang, languageName, publications, catalogVersion, favorites, toast, push } = useApp();
  const [lists, setLists] = useState<HomeLists | null | undefined>(undefined);
  const { activate } = useEntryAction();
  const favoritePubs = inLanguage(publications, lang).filter((p) => favorites.includes(p.dir));
  // Favorites that are not downloaded come from the catalog, if one is cached.
  const missingKeys = favorites.filter((k) => !publications.some((p) => p.dir === k));
  const [extra, setExtra] = useState<CatalogEntry[]>([]);
  useEffect(() => {
    if (missingKeys.length === 0) return setExtra([]);
    let live = true;
    api
      .favoriteEntries(lang, missingKeys)
      .then((r) => live && setExtra((r ?? []).filter((e) => !e.local)))
      .catch(() => live && setExtra([]));
    return () => {
      live = false;
    };
    // The keys array is rebuilt on every render; its content is what matters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lang, catalogVersion, missingKeys.join("|")]);

  useEffect(() => {
    let live = true;
    api
      .homeLists(lang, isoDate(new Date()))
      .then((l) => live && setLists(l))
      .catch((e) => {
        toast(String(e));
        if (live) setLists(null);
      });
    return () => {
      live = false;
    };
  }, [lang, catalogVersion, toast]);

  // Today's text from a downloaded daily text booklet, if there is one.
  const [today, setToday] = useState<DatedPage | null>(null);
  useEffect(() => {
    let live = true;
    api
      .datedPage(lang, "dailyText", isoDate(new Date()))
      .then((p) => live && setToday(p))
      .catch(() => live && setToday(null));
    return () => {
      live = false;
    };
  }, [lang, publications]);

  const daily = lists?.dailyText;
  return (
    <>
      <AppBar title={t("Home")} />
      <div className="flex-1 overflow-y-auto">
        {today ? (
          <div className="bg-surface px-6 py-10 text-center">
            <button
              className="inline-flex items-center gap-2 text-[1.55rem] font-semibold text-accent hover:underline"
              onClick={() => push({ name: "reader", target: today.target })}
            >
              <CalendarDays size={24} strokeWidth={1.5} />
              {longDate(new Date())}
              <ChevronRight size={22} />
            </button>
            <p className="mx-auto mt-2 max-w-3xl">{themeScripture(today.html)}</p>
          </div>
        ) : (
          <div className="bg-bar/60 px-6 py-8 text-center">
            <h2 className="text-[1.55rem] font-semibold text-accent">{t("Welcome to Pergament")}</h2>
            {daily && (
              <button className="mt-3 text-[1.05rem] text-link hover:underline" onClick={() => void activate(daily)}>
                {t(daily.local ? "Open {title}" : "Download {title}", { title: daily.item.title })}
              </button>
            )}
          </div>
        )}
        <div className="px-5 pb-10">
          <SectionTitle>Favorites</SectionTitle>
          {favoritePubs.length + extra.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {favoritePubs.map((p) => (
                <FavoriteTile key={p.dir} pub={p} />
              ))}
              {extra.map((e) => (
                <CoverTile key={`${e.item.symbol}-${e.item.issue_tag}`} entry={e} />
              ))}
            </div>
          ) : (
            <div className="flex h-[88px] w-[264px] items-center justify-center gap-2 px-6 text-center text-sm text-fg/80 ring-1 ring-line">
              <Star size={16} className="shrink-0" /> {t("Add favorites with the menu of a tile")}
            </div>
          )}

          {lists === null && (
            <>
              <SectionTitle>What's New</SectionTitle>
              <CatalogPrompt />
            </>
          )}
          {lists && (
            <>
              <SectionTitle aside={languageName(lang)}>Teaching Toolbox</SectionTitle>
              <div className="flex flex-col gap-2">
                {toolboxRows(lists.teachingToolbox).map((row, i) => (
                  <div key={i} className="flex flex-wrap gap-x-6 gap-y-2">
                    {row.map((group) => (
                      <div key={group[0].category} className="flex flex-wrap gap-2">
                        {group.map((e) => (
                          <CoverTile key={`${e.item.symbol}-${e.item.issue_tag}`} entry={e} />
                        ))}
                      </div>
                    ))}
                  </div>
                ))}
              </div>
              <SectionTitle aside={languageName(lang)}>What's New</SectionTitle>
              <div className="flex gap-1.5 overflow-x-auto pb-2">
                {lists.whatsNew.map((e) => (
                  <EntryCard key={`${e.item.symbol}-${e.item.issue_tag}`} entry={e} language={languageName(lang)} />
                ))}
              </div>
            </>
          )}
        </div>
      </div>
    </>
  );
}
