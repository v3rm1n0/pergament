import { useEffect, useRef, useState } from "react";
import { useApp } from "@/app";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { api, type SearchHit } from "@/lib/api";
import { bibleFor } from "@/lib/bible";
import { isoDate, longDate } from "@/lib/dates";
import { openBibleReference } from "@/lib/go-to";
import { loadBooks } from "@/lib/note-links";
import { t } from "@/lib/i18n";

/** The global command palette for navigation and quick actions. */
export function CommandPalette() {
  const { publications, push, openTarget, toast, lang, bibles, view } = useApp();
  const [open, setOpen] = useState(false);
  const [shortcutHelp, setShortcutHelp] = useState(false);
  const [query, setQuery] = useState("");
  const [searchHits, setSearchHits] = useState<SearchHit[]>([]);
  const [searchLoading, setSearchLoading] = useState(false);
  const searchRequest = useRef(0);
  const openRef = useRef(open);
  openRef.current = open;

  const close = () => {
    setOpen(false);
    setQuery("");
  };


  useEffect(() => {
    const request = ++searchRequest.current;
    setSearchHits([]);
    setSearchLoading(Boolean(query.trim()));
    if (!query.trim()) return;
    const timer = window.setTimeout(() => {
      api.searchPublications({ query, scope: { kind: "all" }, limit: 8, offset: 0 })
        .then((response) => {
          if (request === searchRequest.current) setSearchHits(response.hits.slice(0, 8));
        })
        .catch(() => undefined)
        .finally(() => {
          if (request === searchRequest.current) setSearchLoading(false);
        });
    }, 250);
    return () => window.clearTimeout(timer);
  }, [query]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && openRef.current) {
        close();
        return;
      }
      if (!event.altKey && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        if (!openRef.current) setOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <>
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen) setOpen(true);
        else close();
      }}
    >
      <DialogContent aria-describedby={undefined} className="top-[18vh] translate-y-0 overflow-hidden p-0">
        <DialogTitle className="sr-only">{t("Command palette")}</DialogTitle>
        <Command loop>
          <CommandInput
            autoFocus
            value={query}
            onValueChange={setQuery}
            placeholder={t("Search commands and publications")}
            aria-label={t("Search commands and publications")}
          />
          <CommandList>
            <CommandEmpty>{t("No matching commands or publications.")}</CommandEmpty>
            <CommandGroup heading={t("Actions")}>
              <CommandItem
                onSelect={() => {
                  close();
                  push({ name: "home" });
                }}
              >
                {t("Open home")}
              </CommandItem>
              <CommandItem
                onSelect={() => {
                  close();
                  push({ name: "library", tab: "publications" });
                }}
              >
                {t("Open library")}
              </CommandItem>
              <CommandItem
                value={`Search all text ${query}`}
                onSelect={() => {
                  close();
                  push({ name: "search", query: query.trim() || undefined });
                }}
              >
                {t("Search all text")}
              </CommandItem>
              <CommandItem
                value={`Open Bible reference ${query}`}
                onSelect={() => {
                  const openDir = view.name === "reader" ? view.target.publication : "dir" in view ? view.dir : null;
                  const found = bibleFor(openDir, publications, lang, bibles);
                  if (!found) {
                    toast(t("No Bible in this language in your library yet"));
                    return;
                  }
                  void loadBooks(found.dir).then((books) => openBibleReference(query, { bible: { dir: found.dir, books }, toast, open: openTarget })).then((opened) => {
                    if (opened) close();
                    else toast(t("No such place in this Bible."));
                  }).catch((e) => toast(t("Cannot open Bible: {error}", { error: String(e) })));
                }}
              >
                {t("Open Bible reference")}
              </CommandItem>
              <CommandItem
                onSelect={() => {
                  close();
                  api.datedPage(lang, "dailyText", isoDate(new Date())).then((page) => {
                    if (page) push({ name: "reader", target: page.target });
                    else toast(t("No daily text for {date} in your library", { date: longDate(new Date()) }));
                  }).catch((e) => toast(t("Cannot open daily text: {error}", { error: String(e) })));
                }}
              >
                {t("Daily Text")}
              </CommandItem>
              <CommandItem onSelect={() => { close(); setShortcutHelp(true); }}>
                {t("Keyboard shortcuts")}
              </CommandItem>
            </CommandGroup>
            {searchLoading && <CommandItem disabled>{t("Searching…")}</CommandItem>}
            {/* Kept mounted while empty: cmdk 1.1 crashes when a group unmounts during a search. */}
            <CommandGroup heading={searchHits.length > 0 ? t("Full-text results") : undefined}>
              {searchHits.map((hit, index) => (
                <CommandItem
                  key={`${hit.target.publication}-${index}`}
                  value={`${hit.title} ${hit.publicationTitle} ${hit.snippet}`}
                  keywords={[query]}
                  onSelect={() => {
                    close();
                    push({ name: "reader", target: hit.target, fromSearch: true });
                  }}
                >
                  <span>
                    <span className="block">{hit.title}</span>
                    <span className="block text-xs text-muted">
                      {hit.publicationTitle} — {hit.snippet}
                    </span>
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
            <CommandGroup heading={t("Publications")}>
              {publications.map((publication) => (
                <CommandItem
                  key={publication.dir}
                  value={publication.title}
                  onSelect={() => {
                    close();
                    push({ name: "publication", dir: publication.dir });
                  }}
                >
                  {publication.title}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
      <Dialog open={shortcutHelp} onOpenChange={setShortcutHelp}>
        <DialogContent aria-describedby={undefined}>
          <DialogTitle>{t("Keyboard shortcuts")}</DialogTitle>
          <dl className="space-y-2 text-sm">
            <div className="flex justify-between"><dt>{t("Open command palette")}</dt><dd>Ctrl+K</dd></div>
            <div className="flex justify-between"><dt>{t("Go to a verse")}</dt><dd>Ctrl+G</dd></div>
            <div className="flex justify-between"><dt>{t("Link reference")}</dt><dd>Ctrl+L</dd></div>
            <div className="flex justify-between"><dt>{t("Open linked reference")}</dt><dd>Ctrl+P</dd></div>
            <div className="flex justify-between"><dt>{t("Pop out note")}</dt><dd>Ctrl+Shift+O</dd></div>
          </dl>
        </DialogContent>
      </Dialog>
    </>
  );
}
