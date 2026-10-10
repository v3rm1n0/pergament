import { useEffect, useState } from "react";
import { FileText, Star } from "lucide-react";
import { AppBar, BarButton, useApp } from "@/app";
import { api, documentTarget, type PubDetail, type TocNode } from "@/lib/api";
import { booksTabIndex, hasBooks, shortBookName } from "@/lib/bible";
import { t } from "@/lib/i18n";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

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

/** One Testament: heading plus its books as a grid that is wide enough for the full short names. */
function BookSection({ dir, node }: { dir: string; node: TocNode }) {
  const { push } = useApp();
  const books = node.children.filter((c) => c.bible_book != null);
  return (
    <section>
      <h2 className="mb-3 text-sm font-semibold">{node.title}</h2>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(136px,1fr))] gap-2">
        {books.map((b) => (
          <button
            key={b.bible_book}
            onClick={() => push({ name: "chapters", dir, book: b.bible_book! })}
            className="flex h-10 items-center rounded-md border border-line px-3 text-left text-sm hover:bg-bar focus-visible:outline-2 focus-visible:outline-accent"
          >
            <span className="truncate">{shortBookName(b.title)}</span>
          </button>
        ))}
      </div>
    </section>
  );
}

function BooksTab({ dir, node }: { dir: string; node: TocNode }) {
  // The books tab holds one child per Testament; they stack, so the books keep their room.
  return (
    <div className="flex flex-col gap-10">
      {node.children.filter(hasBooks).map((s) => (
        <BookSection key={s.title} dir={dir} node={s} />
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
        <div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-2">
          {docs.map((d, i) => (
            <button
              key={`${d.document_id}-${i}`}
              onClick={() => openTarget(documentTarget(dir, d.document_id!))}
              className="flex min-h-12 items-center gap-3 rounded-md border border-line bg-surface px-3 py-2 text-left text-sm hover:bg-bar"
            >
              <FileText className="shrink-0 text-muted" size={18} strokeWidth={1.5} />
              <span className="line-clamp-2">{d.title}</span>
            </button>
          ))}
        </div>
      )}
      {groups.map((g, i) => (
        <section key={`${g.title}-${i}`}>
          <h2 className="mb-3 text-sm font-semibold">{g.title}</h2>
          <DocList dir={dir} nodes={g.children} />
        </section>
      ))}
    </div>
  );
}

/** Star in the top bar that adds the publication to the Home favorites. */
function FavoriteButton({ dir }: { dir: string }) {
  const { favorites, toggleFavorite } = useApp();
  const on = favorites.includes(dir);
  return (
    <BarButton label={on ? "Remove from favorites" : "Add to favorites"} onClick={() => toggleFavorite(dir)}>
      <Star size={18} strokeWidth={1.6} className={on ? "fill-accent text-accent" : undefined} />
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
        <div className="page flex-1 overflow-y-auto">
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
        <div className="page pb-0 pt-6">
          <TabsList>
            {toc.map((t, i) => (
              <TabsTrigger key={i} value={String(i)}>
                {t.title}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        {toc.map((t, i) => (
          <TabsContent key={i} value={String(i)} className="page flex-1 pt-6 overflow-y-auto outline-none">
            {hasBooks(t) ? <BooksTab dir={dir} node={t} /> : <DocList dir={dir} nodes={t.children} />}
          </TabsContent>
        ))}
      </Tabs>
    </>
  );
}
