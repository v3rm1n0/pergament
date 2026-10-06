import { useEffect, useState } from "react";
import { FileText, Star } from "lucide-react";
import { AppBar, BarButton, useApp } from "@/app";
import { loadFavorites, toggleFavorite } from "@/lib/settings";
import { api, documentTarget, type PubDetail, type TocNode } from "@/lib/api";
import { bookShade, booksTabIndex, hasBooks, shortBookName } from "@/lib/bible";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

const shadeClass = ["bg-tile-0", "bg-tile-1", "bg-tile-2"] as const;

export function usePublication(dir: string): PubDetail | null {
  const { toast } = useApp();
  const [detail, setDetail] = useState<PubDetail | null>(null);
  useEffect(() => {
    let live = true;
    api
      .publication(dir)
      .then((d) => live && setDetail(d))
      .catch((e) => toast(t("Cannot open publication: {error}", { error: String(e) })));
    return () => {
      live = false;
    };
  }, [dir, toast]);
  return detail;
}

/** One Testament: heading plus the book tile grid. */
function BookSection({ dir, node, columns }: { dir: string; node: TocNode; columns: number }) {
  const { push } = useApp();
  const books = node.children.filter((c) => c.bible_book != null);
  return (
    <section className="min-w-0 flex-1">
      <h2 className="mb-4 text-[1.65rem] font-bold uppercase leading-tight tracking-wide">{node.title}</h2>
      <div className="grid gap-[3px]" style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>
        {books.map((b) => (
          <button
            key={b.bible_book}
            onClick={() => push({ name: "chapters", dir, book: b.bible_book! })}
            className={cn(
              "flex h-[4.5rem] items-center px-3.5 text-left text-[1.05rem] text-tile-fg transition-[filter] hover:brightness-125",
              shadeClass[bookShade(b.bible_book!)],
            )}
          >
            <span className="truncate">{shortBookName(b.title)}</span>
          </button>
        ))}
      </div>
    </section>
  );
}

function BooksTab({ dir, node }: { dir: string; node: TocNode }) {
  // The books tab holds one child per Testament.
  const sections = node.children.filter(hasBooks);
  return (
    <div className="flex flex-col gap-10 lg:flex-row lg:gap-8">
      {sections.map((s, i) => (
        <BookSection key={s.title} dir={dir} node={s} columns={i === 0 && sections.length > 1 ? 4 : 3} />
      ))}
    </div>
  );
}

/** Documents of a TOC subtree as a tile list; nested groups get headings. */
function DocList({ dir, nodes }: { dir: string; nodes: TocNode[] }) {
  const { openTarget } = useApp();
  const docs = nodes.filter((n) => n.document_id != null && n.children.length === 0);
  const groups = nodes.filter((n) => n.children.length > 0);
  return (
    <div className="space-y-8">
      {docs.length > 0 && (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-[3px]">
          {docs.map((d, i) => (
            <button
              key={`${d.document_id}-${i}`}
              onClick={() => openTarget(documentTarget(dir, d.document_id!))}
              className="flex min-h-[4.5rem] items-center gap-3 bg-surface px-4 py-3 text-left ring-1 ring-line hover:bg-bar"
            >
              <FileText className="shrink-0 text-muted" size={20} strokeWidth={1.5} />
              <span className="line-clamp-2">{d.title}</span>
            </button>
          ))}
        </div>
      )}
      {groups.map((g, i) => (
        <section key={`${g.title}-${i}`}>
          <h2 className="mb-3 text-lg font-bold uppercase tracking-wide">{g.title}</h2>
          <DocList dir={dir} nodes={g.children} />
        </section>
      ))}
    </div>
  );
}

/** Star in the top bar that adds the publication to the Home favorites. */
function FavoriteButton({ dir }: { dir: string }) {
  const [on, setOn] = useState(() => loadFavorites().includes(dir));
  return (
    <BarButton label={on ? "Remove from favorites" : "Add to favorites"} onClick={() => setOn(toggleFavorite(dir).includes(dir))}>
      <Star size={21} strokeWidth={1.5} className={on ? "fill-accent text-accent" : undefined} />
    </BarButton>
  );
}

export function PublicationView({ dir, tab }: { dir: string; tab?: number }) {
  const { replace } = useApp();
  const { lang, languageName } = useApp();
  const detail = usePublication(dir);
  if (!detail) return <AppBar title="" />;
  const title = detail.card.shortTitle ?? detail.card.title;
  const toc = detail.toc;

  // A single root (e.g. a magazine issue) has no tabs; list its documents.
  if (toc.length <= 1) {
    return (
      <>
        <AppBar title={title} subtitle={languageName(lang)}>
          <FavoriteButton dir={dir} />
        </AppBar>
        <div className="flex-1 overflow-y-auto px-8 py-8">
          <DocList dir={dir} nodes={toc[0]?.children ?? []} />
        </div>
      </>
    );
  }

  const active = String(tab ?? booksTabIndex(toc));
  return (
    <>
      <AppBar title={title} subtitle={languageName(lang)}>
        <FavoriteButton dir={dir} />
      </AppBar>
      <Tabs
        value={active}
        onValueChange={(v) => replace({ name: "publication", dir, tab: Number(v) })}
        className="flex min-h-0 flex-1 flex-col"
      >
        <TabsList>
          {toc.map((t, i) => (
            <TabsTrigger key={i} value={String(i)}>
              {t.title}
            </TabsTrigger>
          ))}
        </TabsList>
        {toc.map((t, i) => (
          <TabsContent key={i} value={String(i)} className="flex-1 overflow-y-auto px-8 py-8 outline-none">
            {hasBooks(t) ? <BooksTab dir={dir} node={t} /> : <DocList dir={dir} nodes={t.children} />}
          </TabsContent>
        ))}
      </Tabs>
    </>
  );
}
