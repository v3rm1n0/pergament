import { useEffect, useRef, useState } from "react";
import { useApp } from "@/app";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { api, type SearchHit } from "@/lib/api";
import { t } from "@/lib/i18n";

/** The global command palette for navigation and quick actions. */
export function CommandPalette() {
  const { publications, push } = useApp();
  const [open, setOpen] = useState(false);
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
  );
}
