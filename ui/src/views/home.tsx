import { useEffect, useState } from "react";
import { Star } from "lucide-react";
import { AppBar, SectionTitle, useApp } from "@/app";
import { api, type CatalogEntry, type DatedPage, type HomeLists } from "@/lib/api";
import { isoDate, longDate } from "@/lib/dates";
import { themeScriptureParts } from "@/lib/page";
import { inLanguage } from "@/lib/settings";
import { t } from "@/lib/i18n";
import { TileMenu, entryTarget } from "@/components/tile-menu";
import { CatalogPrompt, EntryImage, useEntryAction } from "@/components/catalog";
import { EntryCover, PubCover } from "@/components/cover";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Progress } from "@/components/ui/progress";

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

/** One What's New row: thumbnail, title, category and a small button that downloads or opens the entry. */
function NewRow({ entry }: { entry: CatalogEntry }) {
  const { lang } = useApp();
  const { activate, progress } = useEntryAction();
  const p = progress(entry);
  const title = entry.item.issue_title || entry.item.title;
  const action = t(entry.local ? "Open" : "Get");
  return (
    <li className="group relative flex items-center gap-3 border-b border-line px-1 py-2 hover:bg-bar">
      <EntryImage entry={entry} className="aspect-[4/5] w-9 shrink-0 rounded-[6px] border border-line" />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm leading-normal">{title}</div>
        {entry.category && <div className="truncate text-[13px] leading-normal text-muted">{t(entry.category)}</div>}
        {p && <Progress value={p.total ? (p.done / p.total) * 100 : null} label={title} className="mt-1 h-0.5" />}
      </div>
      <Button
        variant="outline"
        size="sm"
        className="h-7 px-2.5 text-[13px]"
        disabled={!!p}
        aria-label={`${action} · ${title}`}
        onClick={() => void activate(entry)}
      >
        {action}
      </Button>
      <span className="relative size-7 shrink-0">
        <TileMenu
          target={entryTarget(entry, lang)}
          className="right-0 top-0 size-7 border-transparent bg-transparent text-muted opacity-100 hover:text-fg"
        />
      </span>
    </li>
  );
}

export function HomeView() {
  const { lang, languageName, publications, libraryReady, catalogVersion, favorites, toast, push } = useApp();
  const [lists, setLists] = useState<HomeLists | null | undefined>(undefined);
  const { activate } = useEntryAction();
  const favoritePubs = inLanguage(publications, lang).filter((p) => favorites.includes(p.dir));
  // Favorites that are not downloaded come from the catalog, if one is cached. A key that is not a library
  // directory name (non-Latin symbols are cleaned there) can still resolve to a downloaded entry, which shows too.
  const missingKeys = favorites.filter((k) => !publications.some((p) => p.dir === k));
  const [extra, setExtra] = useState<CatalogEntry[]>([]);
  useEffect(() => {
    // Until the library is read every favorite looks missing.
    if (!libraryReady || missingKeys.length === 0) return setExtra([]);
    let live = true;
    api
      .favoriteEntries(lang, missingKeys)
      .then((r) => live && setExtra(r ?? []))
      .catch(() => live && setExtra([]));
    return () => {
      live = false;
    };
    // The keys array is rebuilt on every render; its content is what matters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lang, catalogVersion, libraryReady, missingKeys.join("|")]);

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
  const scripture = today ? themeScriptureParts(today.html) : null;
  const groups = lists ? toolboxRows(lists.teachingToolbox).flat() : [];
  const categories = groups.filter((g) => g[0].category != null);
  const [tab, setTab] = useState("all");
  // A category that the new language does not have falls back to All.
  const active = categories.some((g) => g[0].category === tab) ? tab : "all";
  const shownGroups = active === "all" ? groups : groups.filter((g) => g[0].category === active);
  return (
    <>
      <AppBar title={t("Home")} />
      <div className="flex-1 overflow-y-auto bg-surface">
        <div className="mx-auto flex max-w-[880px] flex-col gap-10 px-4 py-8 min-[720px]:px-6">
          {today && scripture ? (
            <section>
              <p className="text-[13px] text-muted">{t("Today")}</p>
              <h2 className="mt-1 text-2xl font-semibold leading-tight">{longDate(new Date())}</h2>
              <p className="mt-3 max-w-[60ch] text-[19px] leading-[1.6] text-fg [text-wrap:pretty] min-[720px]:text-[20px]">
                {scripture.text}
              </p>
              {scripture.reference && <p className="mt-1 text-[13px] text-accent">{scripture.reference}</p>}
              <div className="mt-4 flex gap-2">
                <Button onClick={() => push({ name: "reader", target: today.target })}>{t("Read")}</Button>
              </div>
            </section>
          ) : (
            <section>
              <h2 className="text-2xl font-semibold leading-tight">{t("Welcome to Pergament")}</h2>
              {daily && (
                <Button variant="outline" className="mt-3" onClick={() => void activate(daily)}>
                  {t(daily.local ? "Open {title}" : "Download {title}", { title: daily.item.title })}
                </Button>
              )}
            </section>
          )}

          <section>
            <SectionTitle variant="quiet">Favorites</SectionTitle>
            {favoritePubs.length + extra.length > 0 ? (
              <div className="flex flex-wrap gap-x-3 gap-y-4">
                {favoritePubs.map((p) => (
                  <PubCover key={p.dir} pub={p} />
                ))}
                {extra.map((e) => (
                  <EntryCover key={`${e.item.symbol}-${e.item.issue_tag}`} entry={e} />
                ))}
              </div>
            ) : (
              <div className="flex items-center gap-2 rounded-md border border-dashed border-line px-4 py-5 text-[13px] leading-normal text-muted">
                <Star size={16} className="shrink-0" /> {t("Add favorites with the menu of a tile")}
              </div>
            )}
          </section>

          {lists === null && (
            <section>
              <SectionTitle variant="quiet">What's New</SectionTitle>
              <CatalogPrompt />
            </section>
          )}
          {lists && (
            <>
              <section>
                <SectionTitle variant="quiet" aside={languageName(lang)}>
                  Teaching Toolbox
                </SectionTitle>
                <Tabs value={active} onValueChange={setTab}>
                  {categories.length > 1 && (
                    <TabsList className="mb-4">
                      <TabsTrigger value="all">{t("All")}</TabsTrigger>
                      {categories.map((g) => (
                        <TabsTrigger key={g[0].category} value={g[0].category!}>
                          {t(g[0].category!)}
                        </TabsTrigger>
                      ))}
                    </TabsList>
                  )}
                  <TabsContent value={active} className="flex flex-wrap gap-x-3 gap-y-4">
                    {shownGroups.flat().map((e) => (
                      <EntryCover key={`${e.item.symbol}-${e.item.issue_tag}`} entry={e} />
                    ))}
                  </TabsContent>
                </Tabs>
              </section>
              <section>
                <SectionTitle variant="quiet" aside={languageName(lang)}>
                  What's New
                </SectionTitle>
                <ul className="border-t border-line">
                  {lists.whatsNew.map((e) => (
                    <NewRow key={`${e.item.symbol}-${e.item.issue_tag}`} entry={e} />
                  ))}
                </ul>
              </section>
            </>
          )}
        </div>
      </div>
    </>
  );
}
