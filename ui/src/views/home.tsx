import { useEffect, useState } from "react";
import { BookOpen, Star } from "lucide-react";
import { AppBar, SectionTitle, useApp } from "@/app";
import { api, type HomeLists, type PubCard } from "@/lib/api";
import { isoDate } from "@/lib/dates";
import { inLanguage, loadFavorites } from "@/lib/settings";
import { CatalogPrompt, CoverTile, EntryCard, useEntryAction } from "@/components/catalog";

function FavoriteTile({ pub }: { pub: PubCard }) {
  const { push } = useApp();
  return (
    <button
      onClick={() => push({ name: "publication", dir: pub.dir })}
      className="w-[88px] text-left"
      title={pub.title}
    >
      {pub.cover ? (
        <img src={pub.cover} alt="" className="h-[88px] w-[88px] object-cover" draggable={false} />
      ) : (
        <div className="flex h-[88px] w-[88px] items-center justify-center bg-tile">
          <BookOpen size={28} strokeWidth={1.2} />
        </div>
      )}
      <div className="mt-1 line-clamp-2 text-[0.68rem] leading-tight">{pub.shortTitle ?? pub.title}</div>
    </button>
  );
}

export function HomeView() {
  const { lang, languageName, publications, catalogVersion, toast } = useApp();
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

  const daily = lists?.dailyText;
  return (
    <>
      <AppBar title="Home" />
      <div className="flex-1 overflow-y-auto">
        <div className="bg-bar/60 px-6 py-8 text-center">
          <h2 className="text-[1.55rem] font-semibold text-accent">Welcome to jwlinux</h2>
          {daily && (
            <button className="mt-3 text-[1.05rem] text-link hover:underline" onClick={() => void activate(daily)}>
              {daily.local ? "Open" : "Download"} {daily.item.title}
            </button>
          )}
        </div>
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
              <Star size={16} className="shrink-0" /> Add favorites with the star in a publication
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
