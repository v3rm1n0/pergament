import { useEffect, useState } from "react";
import { Star } from "lucide-react";
import { AppBar, SectionTitle, useApp } from "@/app";
import { api, samePublication, type CatalogEntry, type DatedPage, type HomeLists, type LanguageEntry, type WhatsNewRow } from "@/lib/api";
import { isoDate, longDate } from "@/lib/dates";
import { themeScriptureParts } from "@/lib/page";
import { loadToolboxLang, loadWhatsNewLangs, saveToolboxLang, saveWhatsNewLangs } from "@/lib/settings";
import { t } from "@/lib/i18n";
import { TileMenu, cardTarget, entryTarget } from "@/components/tile-menu";
import { SectionLanguagePicker } from "@/components/language-menu";
import { CatalogPrompt, EntryImage, useEntryAction } from "@/components/catalog";
import { WhatsNewLanguages } from "@/components/whats-new-languages";
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
function NewRow({ row, showLang }: { row: WhatsNewRow; showLang: boolean }) {
  const { languageName } = useApp();
  const { activate, progress } = useEntryAction();
  const p = progress(row);
  const title = row.item.issue_title || row.item.title;
  const action = t(row.local ? "Open" : "Get");
  const muted = [row.category && t(row.category), showLang ? languageName(row.langCode) : null]
    .filter(Boolean)
    .join(" · ");
  return (
    <li className="group relative flex items-center gap-3 border-b border-line px-1 py-2 hover:bg-bar">
      <EntryImage entry={row} className="aspect-[4/5] w-9 shrink-0 rounded-[6px] border border-line" />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm leading-normal">{title}</div>
        {muted && <div className="truncate text-[13px] leading-normal text-muted">{muted}</div>}
        {p && <Progress value={p.total ? (p.done / p.total) * 100 : null} label={title} className="mt-1 h-0.5" />}
      </div>
      <Button
        variant="outline"
        size="sm"
        className="h-7 px-2.5 text-[13px]"
        disabled={!!p}
        aria-label={`${action} · ${title}`}
        onClick={() => void activate(row, row.langCode)}
      >
        {action}
      </Button>
      <span className="relative size-7 shrink-0">
        <TileMenu
          target={entryTarget(row, row.langCode)}
          className="right-0 top-0 size-7 border-transparent bg-transparent text-muted opacity-100 hover:text-fg"
        />
      </span>
    </li>
  );
}

/** The last loaded lists and daily text. The view remounts on every tab switch, so these keep its sections from vanishing until the reload finishes. */
let cachedLists: { lang: string; lists: HomeLists | null } | undefined;
let cachedToday: { lang: string; page: DatedPage | null } | undefined;
let cachedToolbox: { lang: string; entries: CatalogEntry[] | null } | undefined;
/** The last What's New rows for the selected languages, keyed by the joined codes. */
let cachedNew: { key: string; rows: WhatsNewRow[] | null } | undefined;

export function HomeView() {
  const { lang, publications, catalogVersion, favorites, toast, push } = useApp();
  const [lists, setLists] = useState<HomeLists | null | undefined>(
    cachedLists?.lang === lang ? cachedLists.lists : undefined,
  );
  // The Teaching Toolbox language: the picked override, or the app language when `null`.
  const [toolboxLang, setToolboxLang] = useState<string | null>(loadToolboxLang);
  const toolboxEff = toolboxLang ?? lang;
  const [toolboxState, setToolboxState] = useState<CatalogEntry[] | null | undefined>(
    cachedToolbox?.lang === toolboxEff ? cachedToolbox.entries : undefined,
  );
  const { activate } = useEntryAction();
  const missingKeys = favorites.filter((k) => !publications.some((p) => samePublication(k, cardTarget(p, lang))));
  const [extra, setExtra] = useState<LanguageEntry[]>([]);
  useEffect(() => {
    let live = true;
    Promise.all(missingKeys.map((key) => api.favoriteEntry(key).catch((e) => {
      if (live) toast(String(e));
      return null;
    })))
      .then((rows) => { if (live) setExtra(rows.filter((r) => r !== null)); });
    return () => {
      live = false;
    };
    // The keys array is rebuilt on every render; its content is what matters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [catalogVersion, JSON.stringify(missingKeys), toast]);

  const favoriteTiles = favorites.map((key) => {
    const pub = publications.find((p) => samePublication(key, cardTarget(p, lang)));
    const id = JSON.stringify(key);
    if (pub) return <PubCover key={id} pub={pub} />;
    const row = extra.find((r) => samePublication(key, entryTarget(r.entry, r.code)));
    return row ? <EntryCover key={id} entry={row.entry} lang={row.code} /> : null;
  }).filter((tile) => tile !== null);

  useEffect(() => {
    let live = true;
    api
      .homeLists(lang, isoDate(new Date()))
      .then((l) => {
        cachedLists = { lang, lists: l };
        if (live) setLists(l);
      })
      .catch((e) => {
        toast(String(e));
        cachedLists = undefined;
        if (live) setLists(null);
      });
    return () => {
      live = false;
    };
  }, [lang, catalogVersion, toast]);

  // An override fetches its toolbox on its own; the app-language toolbox comes with home_lists.
  useEffect(() => {
    if (toolboxLang == null) return;
    let live = true;
    api
      .teachingToolbox(toolboxEff)
      .then((entries) => {
        cachedToolbox = { lang: toolboxEff, entries };
        if (live) setToolboxState(entries);
      })
      .catch((e) => {
        toast(String(e));
        cachedToolbox = undefined;
        if (live) setToolboxState(null);
      });
    return () => {
      live = false;
    };
  }, [toolboxEff, catalogVersion, toast]);

  // Today's text from a downloaded daily text booklet, if there is one.
  const [today, setToday] = useState<DatedPage | null>(cachedToday?.lang === lang ? cachedToday.page : null);
  useEffect(() => {
    let live = true;
    api
      .datedPage(lang, "dailyText", isoDate(new Date()))
      .then((p) => {
        cachedToday = { lang, page: p };
        if (live) setToday(p);
      })
      .catch(() => {
        cachedToday = undefined;
        if (live) setToday(null);
      });
    return () => {
      live = false;
    };
  }, [lang, publications]);

  // Languages of the What's New section; nothing stored, or every code pruned away as retired,
  // means it follows the app language.
  const [picked, setPicked] = useState<string[] | null>(() => loadWhatsNewLangs());
  const wnLangs = picked?.length ? picked : [lang];
  const wnKey = wnLangs.join(",");
  const [newRows, setNewRows] = useState<WhatsNewRow[] | null | undefined>(
    cachedNew?.key === wnKey ? cachedNew.rows : undefined,
  );
  useEffect(() => {
    let live = true;
    api
      .whatsNew(wnKey.split(","))
      .then((r) => {
        cachedNew = { key: wnKey, rows: r };
        if (live) setNewRows(r);
      })
      .catch((e) => {
        // Keep the previous rows; the next catalog or language change retries. With nothing to
        // keep (the first load failed), an empty section still renders: the picker must stay
        // reachable so a dead stored language can be pruned. The failure is not cached.
        toast(String(e));
        if (live) setNewRows((prev) => (prev === undefined ? [] : prev));
      });
    return () => {
      live = false;
    };
  }, [wnKey, catalogVersion, toast]);

  const daily = lists?.dailyText;
  const scripture = today ? themeScriptureParts(today.html) : null;
  const toolboxEntries = toolboxLang == null ? lists?.teachingToolbox : toolboxState;
  const groups = toolboxEntries ? toolboxRows(toolboxEntries).flat() : [];
  const categories = groups.filter((g) => g[0].category != null);
  const [tab, setTab] = useState("all");
  // A category that the new language does not have falls back to All.
  const active = categories.some((g) => g[0].category === tab) ? tab : "all";
  const shownGroups = active === "all" ? groups : groups.filter((g) => g[0].category === active);
  return (
    <>
      <AppBar title={t("Home")} />
      <div className="flex-1 overflow-y-auto bg-surface">
        <div className="mx-auto flex max-w-[var(--page-max,880px)] flex-col gap-10 px-4 py-8 min-[720px]:px-6">
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
            {favoriteTiles.length > 0 ? (
              <div className="flex flex-wrap gap-x-3 gap-y-4">
                {favoriteTiles}
              </div>
            ) : (
              <div className="flex items-center gap-2 rounded-md border border-dashed border-line px-4 py-5 text-[13px] leading-normal text-muted">
                <Star size={16} className="shrink-0" /> {t("Add favorites with the menu of a tile")}
              </div>
            )}
          </section>

          {lists && (
            <section>
              <SectionTitle
                variant="quiet"
                aside={
                  <SectionLanguagePicker
                    value={toolboxLang}
                    fallback={lang}
                    onChange={(c) => {
                      const next = c === lang ? null : c;
                      setToolboxLang(next);
                      saveToolboxLang(next);
                    }}
                  />
                }
              >
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
                    <EntryCover key={`${e.item.symbol}-${e.item.issue_tag}`} entry={e} lang={toolboxEff} />
                  ))}
                </TabsContent>
              </Tabs>
            </section>
          )}
          {newRows !== undefined && (
            <section>
              <SectionTitle
                variant="quiet"
                aside={
                  <WhatsNewLanguages
                    value={wnLangs}
                    onChange={(c) => {
                      setPicked(c);
                      saveWhatsNewLangs(c);
                    }}
                  />
                }
              >
                What's New
              </SectionTitle>
              {newRows === null ? (
                <CatalogPrompt />
              ) : (
                <ul className="border-t border-line">
                  {newRows.map((row) => (
                    <NewRow
                      key={`${row.item.symbol}-${row.item.issue_tag}-${row.langCode}`}
                      row={row}
                      showLang={wnLangs.length > 1}
                    />
                  ))}
                </ul>
              )}
            </section>
          )}
        </div>
      </div>
    </>
  );
}
