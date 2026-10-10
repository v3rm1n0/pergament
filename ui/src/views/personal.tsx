import { useCallback, useEffect, useState } from "react";
import { Bookmark as BookmarkIcon, DatabaseBackup, FolderOpen, Pencil, Plus, Tag } from "lucide-react";
import { AppBar, SectionTitle, useApp } from "@/app";
import { api, type Bookmark, type Loc, type Note, type TagInfo, type UserDataSummary } from "@/lib/api";
import { cn } from "@/lib/utils";
import { NoteCard, NoteEditor } from "@/components/notes";
import { Input } from "@/components/ui/input";
import { HIGHLIGHT_COLORS } from "@/lib/marks";
import { filterNotes, type NoteSort } from "@/lib/notes";
import { t } from "@/lib/i18n";

/** Notes, tags and bookmarks from user data, plus backups. */
export function PersonalView() {
  const { push, toast, userVersion, createBackup, restoreBackup } = useApp();
  const [summary, setSummary] = useState<UserDataSummary | null>(null);
  const [tags, setTags] = useState<TagInfo[]>([]);
  const [tag, setTag] = useState<number | null>(null);
  const [notes, setNotes] = useState<Note[]>([]);
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  const [editing, setEditing] = useState<Note | null>(null);
  const [query, setQuery] = useState("");
  const [color, setColor] = useState<number | null>(null);
  const [sort, setSort] = useState<NoteSort>("newest");
  const shown = filterNotes(notes, { query, color, sort });
  const [version, setVersion] = useState(0);
  const reload = useCallback(() => setVersion((v) => v + 1), []);

  useEffect(() => {
    let live = true;
    Promise.all([api.userDataSummary(), api.tags(), api.allNotes(tag), api.bookmarks()])
      .then(([s, t, n, b]) => {
        if (!live) return;
        setSummary(s);
        setTags(t);
        setNotes(n);
        setBookmarks(b);
      })
      .catch((e) => toast(String(e)));
    return () => {
      live = false;
    };
  }, [tag, version, userVersion, toast]);

  const open = async (loc: Loc) => {
    try {
      const target = await api.openLocation(loc);
      if (target) push({ name: "reader", target });
      else toast(t("This publication is not in your library"));
    } catch (e) {
      toast(String(e));
    }
  };

  return (
    <>
      <AppBar title={t("Personal Study")} subtitle={summary ? summaryLine(summary) : undefined} />
      <div className="page flex-1 overflow-y-auto">
        <div>
          <SectionTitle variant="quiet">Backup</SectionTitle>
          <div className="flex flex-wrap gap-2">
            <button
              onClick={() => void createBackup()}
              className="flex items-center gap-2 rounded-md bg-brand px-4 py-2 text-sm text-brand-fg hover:opacity-90 dark:bg-fg dark:text-surface"
            >
              <DatabaseBackup size={17} /> {t("Create backup")}
            </button>
            <button
              onClick={() => void restoreBackup()}
              className="flex items-center gap-2 rounded-md border border-line bg-surface px-4 py-2 text-sm hover:bg-bar"
            >
              <FolderOpen size={17} /> {t("Restore backup…")}
            </button>
          </div>
          <p className="mt-2 text-[13px] text-muted">
            {t("Backups are .jwlibrary files with highlights, notes, tags and bookmarks.")}
          </p>

          <SectionTitle>Notes and Tags</SectionTitle>
          {tags.length > 0 && (
            <div className="mb-4 flex flex-wrap gap-1.5">
              <Chip active={tag === null} onClick={() => setTag(null)}>
                {t("All")}
              </Chip>
              {tags.map((tg) => (
                <Chip key={tg.id} active={tag === tg.id} onClick={() => setTag(tg.id)}>
                  <Tag size={13} /> {tg.name} <span className="text-muted">{tg.notes}</span>
                </Chip>
              ))}
            </div>
          )}
          {notes.length > 0 && (
            <div className="mb-4 flex flex-wrap items-center gap-2">
              <Input
                type="search"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("Search notes")}
                aria-label={t("Search notes")}
                className="w-full max-w-64"
              />
              <div className="flex items-center gap-1" role="group" aria-label={t("Highlight color")}>
                {HIGHLIGHT_COLORS.map((c) => (
                  <button
                    key={c.index}
                    title={t(c.name)}
                    aria-label={t(c.name)}
                    aria-pressed={color === c.index}
                    onClick={() => setColor((cur) => (cur === c.index ? null : c.index))}
                    className={cn(
                      `hl-dot-${c.index} size-5 rounded-full border border-line focus-visible:outline-2 focus-visible:outline-accent`,
                      color === c.index && "ring-2 ring-fg ring-offset-2 ring-offset-surface",
                    )}
                  />
                ))}
              </div>
              <select
                value={sort}
                onChange={(e) => setSort(e.target.value as NoteSort)}
                aria-label={t("Sort notes")}
                className="h-9 rounded-md border border-line bg-surface px-2 text-sm text-fg focus-visible:outline-2 focus-visible:outline-accent"
              >
                <option value="newest">{t("Newest first")}</option>
                <option value="oldest">{t("Oldest first")}</option>
                <option value="publication">{t("By publication")}</option>
              </select>
            </div>
          )}
          {notes.length === 0 ? (
            <p className="flex items-center gap-3 text-sm text-muted">
              <Plus size={20} /> {t("Select text in a publication to highlight it or add a note.")}
            </p>
          ) : shown.length === 0 ? (
            <p className="text-sm text-muted">{t("No notes match.")}</p>
          ) : (
            <div className="flex flex-col gap-2">
              {shown.map((n) => (
                <div key={n.guid} className="flex items-stretch gap-1">
                  <div className="min-w-0 flex-1">
                    <NoteCard
                      note={n}
                      subtitle={n.location?.title ?? undefined}
                      onClick={n.location ? () => void open(n.location!) : () => setEditing(n)}
                    />
                  </div>
                  <button
                    title={t("Edit note")}
                    aria-label={t("Edit note")}
                    onClick={() => setEditing(n)}
                    className="flex w-10 items-center justify-center rounded-md border border-line text-muted hover:bg-bar hover:text-fg"
                  >
                    <Pencil size={16} />
                  </button>
                </div>
              ))}
            </div>
          )}

          {bookmarks.length > 0 && (
            <>
              <SectionTitle>Bookmarks</SectionTitle>
              <div className="flex flex-col gap-1">
                {bookmarks.map((b) => (
                  <button
                    key={`${b.location.keySymbol}-${b.slot}-${b.title}`}
                    onClick={() => void open(b.location)}
                    className="flex items-center gap-3 rounded-md px-1 py-1.5 text-left hover:bg-bar"
                  >
                    <BookmarkIcon size={18} className="shrink-0 text-accent" />
                    <div className="min-w-0">
                      <div className="truncate text-sm">{b.title}</div>
                      {b.snippet && <div className="truncate text-[13px] text-muted">{b.snippet}</div>}
                    </div>
                  </button>
                ))}
              </div>
            </>
          )}
        </div>
      </div>
      {editing && (
        <NoteEditor
          note={editing}
          onClose={() => setEditing(null)}
          onSave={(input) => {
            setEditing(null);
            api
              .saveNote(null, { ...input, guid: editing.guid })
              .then(reload)
              .catch((e) => toast(String(e)));
          }}
          onDelete={() => {
            setEditing(null);
            api
              .deleteNote(editing.guid)
              .then(reload)
              .catch((e) => toast(String(e)));
          }}
        />
      )}
    </>
  );
}

function summaryLine(s: UserDataSummary): string {
  return t("{notes} notes · {marks} highlights · {tags} tags · {bookmarks} bookmarks", {
    notes: s.notes,
    marks: s.marks,
    tags: s.tags,
    bookmarks: s.bookmarks,
  });
}

function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        "flex items-center gap-1 rounded-md border border-line px-2.5 py-1 text-[13px]",
        active ? "bg-bar font-medium" : "bg-surface text-muted hover:bg-bar hover:text-fg",
      )}
    >
      {children}
    </button>
  );
}
