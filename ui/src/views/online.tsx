import { useEffect, useState } from "react";
import { Check, Download, Search } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { AppBar, useApp } from "@/app";
import { api, type CatalogItem, type Progress as ProgressEvent } from "@/lib/api";
import { defaultLangCode } from "@/lib/settings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";

const taskKey = (i: CatalogItem) => `download:${i.symbol}:${i.issue_tag}`;
const mb = (n: number) => `${(n / 1e6).toFixed(1)} MB`;

export function OnlineView() {
  const { toast, refreshPublications, push } = useApp();
  const [lang, setLang] = useState(() => defaultLangCode(navigator.language));
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<CatalogItem[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [progress, setProgress] = useState<Record<string, ProgressEvent>>({});
  const [done, setDone] = useState<Record<string, string>>({});

  useEffect(() => {
    const un = listen<ProgressEvent>("progress", (e) =>
      setProgress((p) => ({ ...p, [e.payload.task]: e.payload })),
    );
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const search = async () => {
    if (!lang.trim()) return;
    setSearching(true);
    try {
      setItems(await api.catalogSearch(lang.trim(), query.trim()));
    } catch (e) {
      toast(String(e));
    } finally {
      setSearching(false);
      setProgress((p) => {
        const { catalog: _, ...rest } = p;
        return rest;
      });
    }
  };

  const download = async (item: CatalogItem) => {
    const key = taskKey(item);
    setProgress((p) => ({ ...p, [key]: { task: key, done: 0, total: item.size } }));
    try {
      const dir = await api.downloadPublication(item, lang.trim());
      setDone((d) => ({ ...d, [key]: dir }));
      toast(`Imported ${item.issue_title || item.title}`, {
        label: "Open",
        run: () => push({ name: "publication", dir }),
      });
      await refreshPublications();
    } catch (e) {
      toast(`Download failed: ${e}`);
    } finally {
      setProgress((p) => {
        const { [key]: _, ...rest } = p;
        return rest;
      });
    }
  };

  const catalog = progress.catalog;
  return (
    <>
      <AppBar title="Search online" />
      <div className="flex-1 overflow-y-auto px-8 py-8">
        <form
          className="mb-6 flex max-w-3xl gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void search();
          }}
        >
          <Input
            aria-label="Language code"
            title="Language code, e.g. X (German) or E (English)"
            value={lang}
            onChange={(e) => setLang(e.target.value)}
            className="w-20 text-center uppercase"
          />
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
            Searches the public jw.org catalog. The first search downloads it (about 58 MB); downloads are
            verified and then imported into your library.
          </p>
        ) : items.length === 0 ? (
          <p className="text-muted">Nothing found.</p>
        ) : (
          <div className="grid max-w-5xl gap-[3px]">
            {items.map((item) => {
              const key = taskKey(item);
              const p = progress[key];
              return (
                <div key={key} className="flex items-center gap-4 bg-surface px-4 py-3 ring-1 ring-line">
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">{item.issue_title || item.title}</div>
                    <div className="truncate text-xs text-muted">
                      {item.symbol} · {mb(item.size)}
                      {item.issue_title ? ` · ${item.title}` : ""}
                    </div>
                    {p && <Progress className="mt-2" value={p.total ? (p.done / p.total) * 100 : null} />}
                  </div>
                  {done[key] ? (
                    <Button variant="ghost" size="icon" title="Open" onClick={() => push({ name: "publication", dir: done[key] })}>
                      <Check size={20} className="text-accent" />
                    </Button>
                  ) : (
                    <Button
                      variant="ghost"
                      size="icon"
                      title="Download and import"
                      disabled={!!p}
                      onClick={() => void download(item)}
                    >
                      <Download size={20} />
                    </Button>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </>
  );
}
