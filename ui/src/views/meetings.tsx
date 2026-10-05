import { useEffect, useState } from "react";
import { BookOpen, ChevronLeft, ChevronRight, CloudDownload } from "lucide-react";
import { AppBar, useApp } from "@/app";
import { api, type CatalogEntry, type DatedEntry, type Meetings } from "@/lib/api";
import { addDays, isoDate, rangeLabel, weekLabel, weekStart } from "@/lib/dates";
import { CatalogPrompt, EntryImage, useEntryAction } from "@/components/catalog";
import { Progress } from "@/components/ui/progress";
import { taskKey } from "@/app";

function MaterialLink({ dated }: { dated: DatedEntry }) {
  const { activate } = useEntryAction();
  const e = dated.entry;
  return (
    <button onClick={() => void activate(e)} className="mt-4 flex items-center gap-3 pl-5 text-left text-link hover:underline">
      {e.local ? <BookOpen size={20} strokeWidth={1.5} /> : <CloudDownload size={20} strokeWidth={1.5} />}
      {e.item.issue_title || e.item.title}
    </button>
  );
}

function OtherRow({ entry, subtitle }: { entry: CatalogEntry; subtitle?: string }) {
  const { activate } = useEntryAction();
  const { downloads } = useApp();
  const p = downloads[taskKey(entry)];
  return (
    <button onClick={() => void activate(entry)} className="flex w-full items-center gap-3 py-1.5 text-left">
      <EntryImage entry={entry} className="h-14 w-14 shrink-0" />
      <div className="min-w-0 flex-1">
        {subtitle && <div className="truncate text-[0.8rem] text-fg/85">{entry.item.issue_title || entry.item.title}</div>}
        <div className="truncate text-link hover:underline">{subtitle ?? entry.item.title}</div>
        {p && <Progress className="mt-1.5" value={p.total ? (p.done / p.total) * 100 : null} />}
      </div>
      {entry.local ? <BookOpen size={20} strokeWidth={1.4} /> : <CloudDownload size={20} strokeWidth={1.4} />}
    </button>
  );
}

export function MeetingsView() {
  const { lang, languageName, catalogVersion, toast } = useApp();
  const [week, setWeek] = useState(() => weekStart(new Date()));
  const [data, setData] = useState<Meetings | null | undefined>(undefined);
  const thisWeek = isoDate(weekStart(new Date())) === isoDate(week);

  useEffect(() => {
    let live = true;
    // Mid-week date avoids edge effects at range boundaries.
    api
      .meetings(lang, isoDate(addDays(week, 2)))
      .then((m) => live && setData(m))
      .catch((e) => {
        toast(String(e));
        if (live) setData(null);
      });
    return () => {
      live = false;
    };
  }, [lang, week, catalogVersion, toast]);

  return (
    <>
      <AppBar title="Meetings" subtitle={languageName(lang)} />
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-2xl px-5 pb-12">
          <div className="flex items-center justify-center gap-16 py-3 text-sm">
            <button aria-label="Previous week" onClick={() => setWeek((w) => addDays(w, -7))}>
              <ChevronLeft size={20} />
            </button>
            <span className="w-56 text-center">
              {weekLabel(week)}
              {thisWeek && " · This Week"}
            </span>
            <button aria-label="Next week" onClick={() => setWeek((w) => addDays(w, 7))}>
              <ChevronRight size={20} />
            </button>
          </div>
          {data === null && <CatalogPrompt />}
          {data && (
            <>
              <h2 className="mt-4 text-[1.15rem] font-semibold">Life and Ministry</h2>
              {data.workbook ? (
                <MaterialLink dated={data.workbook} />
              ) : (
                <p className="mt-4 pl-5 text-sm text-muted">No workbook for this week in the catalog.</p>
              )}
              <h2 className="mt-10 text-[1.15rem] font-semibold">Watchtower Study</h2>
              {data.study ? (
                <MaterialLink dated={data.study} />
              ) : (
                <p className="mt-4 pl-5 text-sm text-muted">No study edition for this week in the catalog.</p>
              )}
              <hr className="my-8 border-line" />
              <h2 className="mb-3 text-[1.15rem] font-semibold">Other Meeting Publications</h2>
              {data.workbook && (
                <OtherRow
                  entry={data.workbook.entry}
                  subtitle={`Meetings for ${rangeLabel(data.workbook.start, data.workbook.end)}`}
                />
              )}
              {data.study && (
                <OtherRow
                  entry={data.study.entry}
                  subtitle={`Study Articles for ${rangeLabel(data.study.start, data.study.end)}`}
                />
              )}
              {data.other.map((e) => (
                <OtherRow key={`${e.item.symbol}-${e.item.issue_tag}`} entry={e} />
              ))}
            </>
          )}
        </div>
      </div>
    </>
  );
}
