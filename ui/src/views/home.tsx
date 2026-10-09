import { MediaImg } from "@/components/media-img";
import { useEffect, useState } from "react";
import { BookOpen, CalendarDays, ChevronRight, Star } from "lucide-react";
import { AppBar, SectionTitle, useApp } from "@/app";
import { api, type DatedPage, type HomeLists, type PubCard } from "@/lib/api";
import { isoDate, longDate } from "@/lib/dates";
import { themeScripture } from "@/lib/page";
import { inLanguage, loadFavorites } from "@/lib/settings";
import { t } from "@/lib/i18n";
import { CatalogPrompt, CoverCaption, CoverTile, EntryCard, useEntryAction } from "@/components/catalog";

function FavoriteTile({ pub }: { pub: PubCard }) {
  const { push } = useApp();
  return (
    <button
      onClick={() => push({ name: "publication", dir: pub.dir })}
      className="group w-[88px] text-left"
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
  );
}

export function HomeView() {
  const { lang, languageName, publications, catalogVersion, toast, push } = useApp();
  const [lists, setLists] = useState<HomeLists | null | undefined>(undefined);
  const { activate } = useEntryAction();
  const favorites = loadFavorites();
  const favoritePubs = inLanguage(publications, lang).filter((p) => favorites.includes(p.dir));

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
          {favoritePubs.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {favoritePubs.map((p) => (
                <FavoriteTile key={p.dir} pub={p} />
              ))}
            </div>
          ) : (
            <div className="flex h-[88px] w-[264px] items-center justify-center gap-2 px-6 text-center text-sm text-fg/80 ring-1 ring-line">
              <Star size={16} className="shrink-0" /> {t("Add favorites with the star in a publication")}
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
              <div className="flex flex-wrap gap-2">
                {lists.teachingToolbox.map((e) => (
                  <CoverTile
                    key={`${e.item.symbol}-${e.item.issue_tag}`}
                    entry={e}
                  />
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
