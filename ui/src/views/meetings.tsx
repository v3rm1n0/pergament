import { MediaImg } from "@/components/media-img";
import { useEffect, useState } from "react";
import { BookOpen, ChevronLeft, ChevronRight, CloudDownload } from "lucide-react";
import { AppBar, useApp } from "@/app";
import { api, type CatalogEntry, type DatedEntry, type DatedPage, type Meetings } from "@/lib/api";
import { addDays, isoDate, rangeLabel, weekLabel, weekStart } from "@/lib/dates";
import { CatalogPrompt, EntryImage, useEntryAction } from "@/components/catalog";
import { Progress } from "@/components/ui/progress";
import { t } from "@/lib/i18n";
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

/** This week's part of a downloaded workbook or study edition. */
function WeekItem({ page, caption }: { page: DatedPage; caption?: string }) {
  const { push } = useApp();
  return (
    <button
      onClick={() => push({ name: "reader", target: page.target })}
      className="mt-3 flex w-full items-center gap-3 text-left"
    >
      {page.image ? (
        <MediaImg src={page.image} alt="" className="h-16 w-16 shrink-0 object-cover" draggable={false} />
      ) : (
        <div className="flex h-16 w-16 shrink-0 items-center justify-center bg-tile">
          <BookOpen size={24} strokeWidth={1.3} />
        </div>
      )}
      <div className="min-w-0 flex-1">
        {caption && <div className="text-[0.8rem] uppercase tracking-wide text-fg/80">{caption}</div>}
        <div className="text-link hover:underline">{page.title}</div>
      </div>
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
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-2xl px-5 pb-12">
          <div className="flex items-center justify-center gap-16 py-3 text-sm">
            <button aria-label={t("Previous week")} onClick={() => setWeek((w) => addDays(w, -7))}>
              <ChevronLeft size={20} />
            </button>
            <span className="w-56 text-center">
              {weekLabel(week)}
              {thisWeek && ` · ${t("This Week")}`}
            </span>
            <button aria-label={t("Next week")} onClick={() => setWeek((w) => addDays(w, 7))}>
              <ChevronRight size={20} />
            </button>
          </div>
          {(data !== undefined || local.workbook || local.study) && (
            <>
              <h2 className="mt-4 text-[1.15rem] font-semibold">{t("Life and Ministry")}</h2>
              {local.workbook ? (
                <WeekItem page={local.workbook} />
              ) : data?.workbook ? (
                <MaterialLink dated={data.workbook} />
              ) : (
                <p className="mt-4 pl-5 text-sm text-muted">{t("No workbook for this week.")}</p>
              )}
              <h2 className="mt-10 text-[1.15rem] font-semibold">{t("Watchtower Study")}</h2>
              {local.study ? (
                <WeekItem page={local.study} caption={studyCaption} />
              ) : data?.study ? (
                <MaterialLink dated={data.study} />
              ) : (
                <p className="mt-4 pl-5 text-sm text-muted">{t("No study edition for this week.")}</p>
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
              <hr className="my-8 border-line" />
              <h2 className="mb-3 text-[1.15rem] font-semibold">{t("Other Meeting Publications")}</h2>
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
