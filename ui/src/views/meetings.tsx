import { MediaImg } from "@/components/media-img";
import { useEffect, useState } from "react";
import { BookOpen, ChevronLeft, ChevronRight, CloudDownload } from "lucide-react";
import { AppBar, useApp } from "@/app";
import { api, type CatalogEntry, type DatedEntry, type DatedPage, type Meetings } from "@/lib/api";
import { addDays, isoDate, rangeLabel, weekLabel, weekStart } from "@/lib/dates";
import { CatalogPrompt, EntryImage, useEntryAction } from "@/components/catalog";
import { TileMenu, entryTarget } from "@/components/tile-menu";
import { Progress } from "@/components/ui/progress";
import { t } from "@/lib/i18n";
import { taskKey } from "@/app";

function MaterialLink({ dated }: { dated: DatedEntry }) {
  const { activate } = useEntryAction();
  const e = dated.entry;
  return (
    <button onClick={() => void activate(e)} className="mt-3 flex items-center gap-3 text-left text-sm text-link hover:underline">
      {e.local ? <BookOpen size={20} strokeWidth={1.5} /> : <CloudDownload size={20} strokeWidth={1.5} />}
      {e.item.issue_title || e.item.title}
    </button>
  );
}

/** This week's part of a downloaded workbook or study edition. */
function WeekItem({ page, caption }: { page: DatedPage; caption?: string }) {
  const { push } = useApp();
  return (
    <button
      onClick={() => push({ name: "reader", target: page.target })}
      className="mt-3 flex w-full items-center gap-3 rounded-md text-left focus-visible:outline-2 focus-visible:outline-accent"
    >
      {page.image ? (
        <MediaImg src={page.image} alt="" className="h-16 w-16 shrink-0 rounded-[6px] border border-line object-cover" draggable={false} />
      ) : (
        <div className="flex h-16 w-16 shrink-0 items-center justify-center rounded-[6px] border border-line bg-tile text-muted">
          <BookOpen size={24} strokeWidth={1.3} />
        </div>
      )}
      <div className="min-w-0 flex-1">
        {caption && <div className="text-[13px] text-muted">{caption}</div>}
        <div className="text-sm text-link hover:underline">{page.title}</div>
      </div>
    </button>
  );
}

function OtherRow({ entry, subtitle }: { entry: CatalogEntry; subtitle?: string }) {
  const { activate } = useEntryAction();
  const { downloads, lang } = useApp();
  const p = downloads[taskKey(entry)];
  return (
    <div className="group relative">
      <button onClick={() => void activate(entry)} className="flex w-full items-center gap-3 border-b border-line py-2 pr-20 text-left hover:bg-bar">
        <EntryImage entry={entry} className="aspect-[4/5] w-9 shrink-0 rounded-[6px] border border-line" />
        <div className="min-w-0 flex-1">
          {subtitle && <div className="truncate text-[13px] text-muted">{entry.item.issue_title || entry.item.title}</div>}
          <div className="truncate text-sm text-link hover:underline">{subtitle ?? entry.item.title}</div>
          {p && <Progress className="mt-1.5 h-0.5" value={p.total ? (p.done / p.total) * 100 : null} />}
        </div>
        <span className="pr-1 text-muted">{entry.local ? <BookOpen size={18} strokeWidth={1.4} /> : <CloudDownload size={18} strokeWidth={1.4} />}</span>
      </button>
      <TileMenu target={entryTarget(entry, lang)} className="right-9 top-1/2 -translate-y-1/2" />
    </div>
  );
}

export function MeetingsView() {
  const { lang, languageName, catalogVersion, toast, publications } = useApp();
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

  // The week's program and study article from downloaded issues; works without a catalog.
  const [local, setLocal] = useState<{ workbook: DatedPage | null; study: DatedPage | null }>({
    workbook: null,
    study: null,
  });
  useEffect(() => {
    let live = true;
    const mid = isoDate(addDays(week, 2));
    const get = (kind: "workbook" | "study") => api.datedPage(lang, kind, mid).catch(() => null);
    void Promise.all([get("workbook"), get("study")]).then(
      ([workbook, study]) => live && setLocal({ workbook, study }),
    );
    return () => {
      live = false;
    };
  }, [lang, week, publications]);
  const studyCaption = `${weekLabel(week)}, ${addDays(week, 6).getFullYear()}`;

  return (
    <>
      <AppBar title={t("Meetings")} subtitle={languageName(lang)} />
      <div className="page flex-1 overflow-y-auto">
        <div>
          <div className="flex items-center justify-between gap-4 text-sm tabular-nums">
            <button className="rounded-md p-1.5 text-muted hover:bg-bar hover:text-fg" aria-label={t("Previous week")} onClick={() => setWeek((w) => addDays(w, -7))}>
              <ChevronLeft size={20} />
            </button>
            <span className="text-center">
              {weekLabel(week)}
              {thisWeek && ` · ${t("This Week")}`}
            </span>
            <button className="rounded-md p-1.5 text-muted hover:bg-bar hover:text-fg" aria-label={t("Next week")} onClick={() => setWeek((w) => addDays(w, 7))}>
              <ChevronRight size={20} />
            </button>
          </div>
          {(data !== undefined || local.workbook || local.study) && (
            <>
              <h2 className="mb-1 mt-8 text-sm font-semibold">{t("Life and Ministry")}</h2>
              {local.workbook ? (
                <WeekItem page={local.workbook} />
              ) : data?.workbook ? (
                <MaterialLink dated={data.workbook} />
              ) : (
                <p className="mt-3 text-sm text-muted">{t("No workbook for this week.")}</p>
              )}
              <h2 className="mb-1 mt-10 text-sm font-semibold">{t("Watchtower Study")}</h2>
              {local.study ? (
                <WeekItem page={local.study} caption={studyCaption} />
              ) : data?.study ? (
                <MaterialLink dated={data.study} />
              ) : (
                <p className="mt-3 text-sm text-muted">{t("No study edition for this week.")}</p>
              )}
            </>
          )}
          {data === null && (
            <div className="mt-8">
              <CatalogPrompt />
            </div>
          )}
          {data && (
            <>
              <hr className="my-10 border-line" />
              <h2 className="mb-3 text-sm font-semibold">{t("Other Meeting Publications")}</h2>
              {data.workbook && (
                <OtherRow
                  entry={data.workbook.entry}
                  subtitle={t("Meetings for {range}", { range: rangeLabel(data.workbook.start, data.workbook.end) })}
                />
              )}
              {data.study && (
                <OtherRow
                  entry={data.study.entry}
                  subtitle={t("Study Articles for {range}", { range: rangeLabel(data.study.start, data.study.end) })}
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
