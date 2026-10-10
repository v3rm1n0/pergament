import { useEffect, useRef, useState } from "react";
import { LocateFixed } from "lucide-react";
import { BarButton, useApp } from "@/app";
import { api, chapterTarget } from "@/lib/api";
import { namedBooks, parseReference } from "@/lib/bible";
import { inLanguage } from "@/lib/settings";
import { Button } from "@/components/ui/button";
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
  const { publications, lang, openTarget, toast } = useApp();
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [bible, setBible] = useState<{ dir: string; books: ReturnType<typeof namedBooks> } | null>(null);
  const [missed, setMissed] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const before = useRef<Element | null>(null);

  const show = () => {
    const bibles = inLanguage(publications, lang).filter((p) => p.isBible);
    const found = bibles.find((p) => p.symbol === "nwtsty") ?? bibles[0];
    if (!found) return toast(t("No Bible in this language in your library yet"));
    before.current = document.activeElement;
    setText("");
    setMissed(false);
    setOpen(true);
    api
      .publication(found.dir)
      .then((d) => setBible({ dir: found.dir, books: namedBooks(d.toc, d.books) }))
      .catch((e) => toast(String(e)));
  };
  const close = () => {
    setOpen(false);
    (before.current as HTMLElement | null)?.focus?.();
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "g") {
        e.preventDefault();
        show();
      }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener(GO_TO_EVENT, show);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener(GO_TO_EVENT, show);
    };
  });
  useEffect(() => {
    if (open) input.current?.focus();
  }, [open]);

  if (!open) return null;
  const go = () => {
    const ref = bible && parseReference(text, bible.books);
    if (!bible || !ref) return setMissed(true);
    setOpen(false);
    openTarget(chapterTarget(bible.dir, ref.book, ref.chapter, ref.verse ?? 1));
  };
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[18vh]" onMouseDown={close}>
      <form
        role="dialog"
        aria-modal="true"
        aria-label={t("Go to a verse")}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.key === "Escape" && close()}
        onSubmit={(e) => {
          e.preventDefault();
          go();
        }}
        className="flex w-[min(90vw,420px)] flex-col gap-3 rounded-md border border-line bg-surface p-4 shadow-sm"
      >
        <h2 className="text-sm font-semibold">{t("Go to a verse")}</h2>
        <div className="flex gap-2">
          <Input
            ref={input}
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
    </div>
  );
}
