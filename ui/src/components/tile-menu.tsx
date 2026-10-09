import { useCallback, useEffect, useMemo, useState } from "react";
import { Check, MoreHorizontal } from "lucide-react";
import { useApp } from "@/app";
import { api, type CatalogEntry, type Language, type LanguageEntry, type PubCard } from "@/lib/api";
import { loadLanguages } from "@/components/language-menu";
import { Menu, menuItem } from "@/components/menu";
import { Input } from "@/components/ui/input";
import { mediaLink, pubKey, publicationLink } from "@/lib/share";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";

export interface PubTarget {
  kind: "pub";
  symbol: string;
  issueTag: number;
  mepsLanguage: number;
  langCode: string;
}
export interface MediaTarget {
  kind: "media";
  key: string;
  langCode: string;
}
export type TileTarget = PubTarget | MediaTarget;

/** The menu target of a catalog entry listed in the selected language. */
export const entryTarget = (e: CatalogEntry, lang: string): PubTarget => ({
  kind: "pub",
  symbol: e.item.symbol,
  issueTag: e.item.issue_tag,
  mepsLanguage: e.item.meps_language,
  langCode: lang,
});

/** The menu target of a downloaded publication. */
export const cardTarget = (p: PubCard, lang: string): PubTarget => ({
  kind: "pub",
  symbol: p.symbol,
  issueTag: Number(p.issueTag) || 0,
  mepsLanguage: p.mepsLanguage,
  langCode: p.langCode ?? lang,
});

/** Languages a publication is available in; picking one opens it, downloading it first if needed. */
function LanguagePicker({ target, onClose }: { target: PubTarget; onClose: () => void }) {
  const { push, download } = useApp();
  const [rows, setRows] = useState<LanguageEntry[] | null | undefined>(undefined);
  const [names, setNames] = useState<Language[]>([]);
  const [query, setQuery] = useState("");

  useEffect(() => {
    api
      .catalogLanguages(target.symbol, target.issueTag)
      .then(setRows)
      .catch(() => setRows(null));
    loadLanguages()
      .then(setNames)
      .catch(() => undefined);
  }, [target.symbol, target.issueTag]);

  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);

  const label = useCallback(
    (code: string) => {
      const l = names.find((n) => n.code === code);
      return l ? l.vernacular || l.name : code;
    },
    [names],
  );
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (rows ?? [])
      .map((r) => ({ ...r, name: label(r.code) }))
      .filter((r) => !q || r.name.toLowerCase().includes(q) || r.code.toLowerCase() === q)
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [rows, query, label]);

  const pick = async ({ code, entry }: LanguageEntry) => {
    onClose();
    const dir = entry.local ?? (await download(entry, code));
    if (dir) push({ name: "publication", dir });
  };

  return (
    <>
      <div className="fixed inset-0 z-40 bg-black/40" onClick={onClose} />
      <div
        role="dialog"
        aria-label={t("More Languages")}
        className="fixed left-1/2 top-1/2 z-50 flex max-h-[70vh] w-[320px] max-w-[calc(100vw-16px)] -translate-x-1/2 -translate-y-1/2 flex-col bg-rail shadow-xl ring-1 ring-line"
      >
        <div className="p-3">
          <Input autoFocus value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t("Search languages")} />
        </div>
        <div className="flex-1 overflow-y-auto pb-2">
          {rows === undefined && <div className="px-4 py-2 text-sm text-muted">{t("Loading…")}</div>}
          {rows === null && (
            <div className="px-4 py-2 text-sm text-muted">{t("Load the catalog to see more languages.")}</div>
          )}
          {shown.map((r) => (
            <button
              key={r.code}
              className={cn(menuItem, "flex w-full items-center justify-between")}
              onClick={() => void pick(r)}
            >
              {r.name}
              {r.entry.local && <Check size={15} />}
            </button>
          ))}
        </div>
      </div>
    </>
  );
}

/** The 3-dots button of a tile and its menu. The tile must be `group relative`; this sits in its top-right corner. */
export function TileMenu({
  target,
  onRemove,
  className,
}: {
  target: TileTarget;
  /** Adds a Remove entry (downloaded publications). */
  onRemove?: () => void;
  className?: string;
}) {
  const { toast, favorites, toggleFavorite } = useApp();
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const [picking, setPicking] = useState(false);
  const close = useCallback(() => setAnchor(null), []);
  const key = target.kind === "pub" ? pubKey(target.symbol, target.mepsLanguage, target.issueTag) : null;
  const favorite = key != null && favorites.includes(key);

  const share = async () => {
    close();
    const link =
      target.kind === "pub"
        ? publicationLink(target.symbol, target.issueTag, target.langCode)
        : mediaLink(target.key, target.langCode);
    try {
      await navigator.clipboard.writeText(link);
      toast(t("Link copied"));
    } catch (e) {
      toast(t("Copy failed: {error}", { error: String(e) }));
    }
  };

  return (
    <>
      <button
        aria-label={t("More")}
        aria-haspopup="menu"
        aria-expanded={anchor !== null}
        title={t("More")}
        onClick={(e) => {
          e.stopPropagation();
          const rect = e.currentTarget.getBoundingClientRect();
          setAnchor((a) => (a ? null : rect));
        }}
        className={cn(
          "absolute right-1 top-1 text-white drop-shadow-[0_0_2px_rgba(0,0,0,0.9)] focus-visible:opacity-100 group-hover:opacity-100",
          anchor ? "opacity-100" : "opacity-0",
          className,
        )}
      >
        <MoreHorizontal size={18} strokeWidth={1.8} />
      </button>
      {anchor && (
        <Menu anchor={anchor} onClose={close}>
          <button role="menuitem" className={menuItem} onClick={() => void share()}>
            {t("Share Link")}
          </button>
          {target.kind === "pub" && (
            <button
              role="menuitem"
              className={menuItem}
              onClick={() => {
                close();
                setPicking(true);
              }}
            >
              {t("More Languages")}
            </button>
          )}
          {key != null && (
            <button
              role="menuitem"
              className={menuItem}
              onClick={() => {
                close();
                toggleFavorite(key);
              }}
            >
              {favorite ? t("Remove from Favorites") : t("Add to Favorites")}
            </button>
          )}
          {onRemove && (
            <button
              role="menuitem"
              className={menuItem}
              onClick={() => {
                close();
                onRemove();
              }}
            >
              {t("Remove")}
            </button>
          )}
        </Menu>
      )}
      {picking && target.kind === "pub" && <LanguagePicker target={target} onClose={() => setPicking(false)} />}
    </>
  );
}
