import { useEffect, useRef, useState } from "react";
import { useApp } from "@/app";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { t } from "@/lib/i18n";

/** The global command palette for navigation and quick actions. */
export function CommandPalette() {
  const { publications, push } = useApp();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const openRef = useRef(open);
  openRef.current = open;

  const close = () => {
    setOpen(false);
    setQuery("");
  };

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
                onSelect={() => {
                  close();
                  push({ name: "search" });
                }}
              >
                {t("Search all text")}
              </CommandItem>
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
