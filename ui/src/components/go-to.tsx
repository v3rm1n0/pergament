import { useEffect, useRef, useState } from "react";
import { LocateFixed } from "lucide-react";
import { BarButton, useApp } from "@/app";
import { bibleFor, type Book } from "@/lib/bible";
import { openBibleReference } from "@/lib/go-to";
import { loadBooks } from "@/lib/note-links";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { t } from "@/lib/i18n";

/** Window event that opens the box, for buttons elsewhere. */
export const GO_TO_EVENT = "pergament:goto";

/** Top bar button that opens the box; only a Bible has verses to go to. */
export function GoToButton() {
  return (
    <BarButton label="Go to…" onClick={() => window.dispatchEvent(new Event(GO_TO_EVENT))}>
      <LocateFixed size={18} strokeWidth={1.6} />
    </BarButton>
  );
}

/** The "go to" box: type a reference such as "Heb 10:24" and the reader opens there. Opens with Ctrl+G. */
export function GoTo() {
  const { publications, lang, bibles, view, openTarget, toast } = useApp();
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [bible, setBible] = useState<{ dir: string; books: Book[] } | null>(null);
  const [missed, setMissed] = useState(false);
  // Focus goes back where it was when the box closes, unless it closed by going to the verse.
  const went = useRef(false);

  const isOpen = useRef(false);
  isOpen.current = open;
  const show = () => {
    // Asking again while the box is open must not make it forget where focus has to go back to.
    if (isOpen.current) return;
    // The Bible open in the reader wins, then the one chosen for the language.
    const openDir = view.name === "reader" ? view.target.publication : "dir" in view ? view.dir : null;
    const found = bibleFor(openDir, publications, lang, bibles);
    if (!found) return toast(t("No Bible in this language in your library yet"));
    went.current = false;
    setBible(null);
    setText("");
    setMissed(false);
    setOpen(true);
    loadBooks(found.dir)
      .then((books) => setBible({ dir: found.dir, books }))
      .catch((e) => toast(String(e)));
  };

  // The listeners are set once and call whatever `show` is current.
  const latest = useRef(show);
  latest.current = show;
  useEffect(() => {
    const open = () => latest.current();
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "g") {
        e.preventDefault();
        open();
      }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener(GO_TO_EVENT, open);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener(GO_TO_EVENT, open);
    };
  }, []);
  const go = async () => {
    let error = false;
    const opened = await openBibleReference(text, {
      bible,
      toast: (message) => {
        error = true;
        toast(message);
      },
      open: openTarget,
    });
    if (!opened) {
      if (!error) setMissed(true);
      return;
    }
    went.current = true;
    setOpen(false);
  };
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent
        aria-describedby={undefined}
        onCloseAutoFocus={(e) => went.current && e.preventDefault()}
        className="top-[18vh] translate-y-0 p-4"
      >
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void go();
          }}
          className="flex flex-col gap-3"
        >
          <DialogTitle>{t("Go to a verse")}</DialogTitle>
          <div className="flex gap-2">
            <Input
              autoFocus
              value={text}
              onChange={(e) => {
                setText(e.target.value);
                setMissed(false);
              }}
              placeholder={t("Reference, for example Heb 10:24")}
              aria-label={t("Reference, for example Heb 10:24")}
              aria-invalid={missed}
              className="flex-1"
            />
            <Button type="submit" disabled={!bible}>
              {t("Go")}
            </Button>
          </div>
          {missed && (
            <p role="alert" className="text-[13px] text-muted">
              {t("No such place in this Bible.")}
            </p>
          )}
        </form>
      </DialogContent>
    </Dialog>
  );
}
