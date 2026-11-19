import { useRef, useState } from "react";
import { Bookmark as BookmarkIcon, MoreVertical } from "lucide-react";
import { BarButton, useApp } from "@/app";
import { api, type Bookmark, type BookmarkInput, type Target } from "@/lib/api";
import { t } from "@/lib/i18n";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";

export const BOOKMARK_COLORS = [
  "#b52d69", "#8457a6", "#6495cf", "#4bbcc5", "#169c61",
  "#62d69b", "#c5e65a", "#e58b60", "#c65c5c", "#ad681e",
];

export function BookmarksButton({
  dir,
  title,
  current,
}: {
  dir: string;
  title: string;
  current?: () => { target: Target; position: Omit<BookmarkInput, "slot"> } | null;
}) {
  const { push, toast, notesChanged } = useApp();
  const [open, setOpen] = useState(false);
  const [rows, setRows] = useState<Bookmark[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const position = useRef<ReturnType<NonNullable<typeof current>>>(null);
  const generation = useRef(0);

  const load = async () => {
    const request = ++generation.current;
    setFailed(false);
    setRows(null);
    try {
      const bookmarks = await api.publicationBookmarks(dir);
      if (request === generation.current) setRows(bookmarks);
    } catch (e) {
      if (request === generation.current) {
        setFailed(true);
        toast(String(e));
      }
    }
  };
  const run = async (action: () => Promise<void>, close = true) => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    try {
      await action();
      if (close) setOpen(false);
    } catch (e) {
      toast(String(e));
    } finally {
      running.current = false;
      setBusy(false);
    }
  };
  const save = (slot: number) => run(async () => {
    const saved = position.current;
    if (!saved) return;
    await api.saveBookmark(saved.target, { slot, ...saved.position });
    notesChanged();
  });
  const go = (bookmark: Bookmark) => run(async () => {
    const target = await api.openLocation(bookmark.location);
    if (!target) throw new Error(t("This publication is not in your library"));
    push({ name: "reader", target, bookmark: { blockType: bookmark.blockType, blockIdentifier: bookmark.blockIdentifier } });
  });

  return (
    <>
      <BarButton label="Bookmarks" onClick={() => {
        position.current = current?.() ?? null;
        setOpen(true);
        void load();
      }}>
        <BookmarkIcon size={18} strokeWidth={1.6} />
      </BarButton>
      <Dialog open={open} onOpenChange={(value) => !busy && setOpen(value)}>
        <DialogContent aria-describedby={undefined} className="max-h-[85vh] gap-0 overflow-y-auto p-0">
          <DialogTitle className="border-b border-line px-6 py-5 pr-12">
            {t("Bookmarks")} - {title}
          </DialogTitle>
          {rows === null ? (
            <div className="p-6">
              {failed ? <Button onClick={() => void load()}>{t("Retry")}</Button> : t("Loading…")}
            </div>
          ) : BOOKMARK_COLORS.map((color, slot) => {
            const bookmark = rows.find((b) => b.slot === slot);
            return (
              <div key={slot} className="flex min-h-14 items-stretch border-b border-line">
                <button
                  disabled={busy || (!bookmark && !position.current)}
                  aria-label={bookmark ? t("Open bookmark {slot}: {title}", { slot: slot + 1, title: bookmark.title }) : t("Save bookmark {slot}", { slot: slot + 1 })}
                  onClick={() => void (bookmark ? go(bookmark) : save(slot))}
                  className="flex min-w-0 flex-1 items-center gap-5 px-6 py-3 text-left hover:bg-bar disabled:opacity-50"
                >
                  <BookmarkIcon size={21} fill={color} color={color} className="shrink-0" />
                  <span className="min-w-0">
                    <span className="block truncate text-sm">{bookmark?.title ?? ""}</span>
                    {bookmark?.snippet && <span className="block truncate text-xs text-muted">{bookmark.snippet}</span>}
                  </span>
                </button>
                {bookmark && (
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <button disabled={busy} aria-label={t("Bookmark {slot} options", { slot: slot + 1 })} className="px-4 text-muted hover:bg-bar">
                        <MoreVertical size={18} />
                      </button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                      <DropdownMenuItem disabled={!position.current} onSelect={() => void save(slot)}>
                        {t("Replace with current location")}
                      </DropdownMenuItem>
                      <DropdownMenuItem onSelect={() => void run(async () => {
                        await api.deleteBookmark(dir, slot);
                        notesChanged();
                        setRows((all) => all?.filter((b) => b.slot !== slot) ?? null);
                      }, false)}>
                        {t("Delete")}
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                )}
              </div>
            );
          })}
          <div className="flex justify-end px-4 py-3">
            <Button variant="ghost" disabled={busy} onClick={() => setOpen(false)}>{t("Close")}</Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
