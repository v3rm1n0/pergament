import { useEffect, useState } from "react";
import { Search } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { AppBar, useApp } from "@/app";
import { api, type CatalogEntry, type Progress as ProgressEvent } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { GridCard } from "@/components/catalog";

const mb = (n: number) => `${(n / 1e6).toFixed(1)} MB`;

/** Search the whole catalog in the selected language. */
export function OnlineView() {
  const { toast, lang, languageName, catalogVersion } = useApp();
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<CatalogEntry[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [catalog, setCatalog] = useState<ProgressEvent | null>(null);

  useEffect(() => {
    const un = listen<ProgressEvent>("progress", (e) => {
      if (e.payload.task === "catalog") setCatalog(e.payload);
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const search = async () => {
    setSearching(true);
    try {
      setItems(await api.catalogSearch(lang, query.trim()));
    } catch (e) {
      toast(String(e));
    } finally {
      setSearching(false);
      setCatalog(null);
    }
  };

  // Search again when the language changes or a download finished.
  useEffect(() => {
    if (items !== null) void search();
    // Only these changes should trigger a new search, not every keystroke.
  }, [lang, catalogVersion]);

  return (
    <>
      <AppBar title="Search" subtitle={languageName(lang)} />
      <div className="flex-1 overflow-y-auto px-5 py-6">
        <form
          className="mb-6 flex max-w-3xl gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void search();
          }}
        >
          <Input
            autoFocus
            placeholder="Title or symbol, e.g. Wachtturm Studienausgabe"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="flex-1"
          />
          <Button type="submit" disabled={searching}>
            <Search size={17} /> Search
          </Button>
        </form>

        {catalog && (
          <div className="mb-6 max-w-3xl">
            <div className="mb-1.5 text-sm text-muted">
              Downloading the public catalog… {mb(catalog.done)}
              {catalog.total ? ` / ${mb(catalog.total)}` : ""}
            </div>
            <Progress value={catalog.total ? (catalog.done / catalog.total) * 100 : null} />
          </div>
        )}

        {items === null ? (
          <p className="max-w-3xl text-muted">
            Searches the public jw.org catalog in {languageName(lang)}. The first search downloads the catalog
            (about 58 MB); downloads are verified and then imported into your library.
          </p>
        ) : items.length === 0 ? (
          <p className="text-muted">Nothing found.</p>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(130px,1fr))] gap-x-3 gap-y-5">
            {items.map((e) => (
              <GridCard key={`${e.item.symbol}-${e.item.issue_tag}`} entry={e} />
            ))}
          </div>
        )}
      </div>
    </>
  );
}
