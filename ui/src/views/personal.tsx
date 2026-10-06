import { useCallback, useEffect, useState } from "react";
import { Bookmark as BookmarkIcon, DatabaseBackup, FolderOpen, Pencil, Plus, Tag } from "lucide-react";
import { AppBar, SectionTitle, useApp } from "@/app";
import { api, type Bookmark, type Loc, type Note, type TagInfo, type UserDataSummary } from "@/lib/api";
import { cn } from "@/lib/utils";
import { NoteCard, NoteEditor } from "@/components/notes";
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
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-3xl px-5 pb-12">
          <SectionTitle>Backup</SectionTitle>
          <div className="flex flex-wrap gap-2">
            <button
              onClick={() => void createBackup()}
              className="flex items-center gap-2 bg-brand px-4 py-2 text-sm text-brand-fg hover:brightness-110"
            >
              <DatabaseBackup size={17} /> {t("Create backup")}
            </button>
            <button
              onClick={() => void restoreBackup()}
              className="flex items-center gap-2 bg-tile px-4 py-2 text-sm hover:brightness-110"
            >
              <FolderOpen size={17} /> {t("Restore backup…")}
            </button>
          </div>
          <p className="mt-2 text-xs text-muted">
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
          {notes.length === 0 ? (
            <p className="flex items-center gap-3 text-sm text-muted">
              <Plus size={20} /> {t("Select text in a publication to highlight it or add a note.")}
            </p>
          ) : (
            <div className="flex flex-col gap-2">
              {notes.map((n) => (
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
                    className="flex w-10 items-center justify-center bg-tile/60 hover:text-accent"
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
                    className="flex items-center gap-3 py-1.5 text-left hover:text-accent"
                  >
                    <BookmarkIcon size={18} className="shrink-0 text-accent" />
                    <div className="min-w-0">
                      <div className="truncate">{b.title}</div>
                      {b.snippet && <div className="truncate text-xs text-muted">{b.snippet}</div>}
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
      className={cn(
        "flex items-center gap-1 px-2.5 py-1 text-sm",
        active ? "bg-brand text-brand-fg" : "bg-tile hover:brightness-110",
      )}
    >
      {children}
    </button>
  );
}
