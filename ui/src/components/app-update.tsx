import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useApp } from "@/app";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { api, type AvailableUpdate, type Progress as ProgressEvent } from "@/lib/api";
import { t } from "@/lib/i18n";
import { renderMarkdown } from "@/lib/markdown";

/** How often the dialog looks again at running jobs and unsaved notes before it restarts. */
const POLL_MS = 1000;

type Stage = { name: "offer" } | { name: "installing"; percent: number | null } | { name: "waiting"; jobs: number; unsaved: boolean };

/**
 * "Check for updates" in About: asks GitHub only when clicked. A newer release
 * is shown with its notes; confirming installs it and restarts once no
 * download, import or unsaved note would be lost.
 */
export function AppUpdate() {
  const { toast, hasUnsavedNote } = useApp();
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<AvailableUpdate | null>(null);
  const [stage, setStage] = useState<Stage>({ name: "offer" });

  const check = async () => {
    setChecking(true);
    try {
      const result = await api.checkAppUpdate();
      if (result.kind === "nix") toast(t("Pergament was installed with Nix. Update it through your Nix configuration."));
      else if (!result.available) toast(t("Pergament is up to date"));
      else {
        setStage({ name: "offer" });
        setUpdate(result.available);
      }
    } catch (e) {
      toast(t("Could not check for updates: {error}", { error: String(e) }));
    } finally {
      setChecking(false);
    }
  };

  const install = async (version: string) => {
    setStage({ name: "installing", percent: null });
    try {
      await api.installAppUpdate(version);
    } catch (e) {
      setUpdate(null);
      toast(t("Could not install the update: {error}", { error: String(e) }));
      return;
    }
    setStage({ name: "waiting", jobs: 0, unsaved: false });
  };

  const installing = stage.name === "installing";
  useEffect(() => {
    if (!installing) return;
    const un = listen<ProgressEvent>("progress", (e) => {
      const { task, done, total } = e.payload;
      if (task === "app-update") setStage({ name: "installing", percent: total ? Math.round((done * 100) / total) : null });
    });
    return () => void un.then((f) => f());
  }, [installing]);

  // Installed: restart as soon as nothing would be lost.
  const waiting = stage.name === "waiting";
  useEffect(() => {
    if (!waiting) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const look = async () => {
      const unsaved = hasUnsavedNote();
      const jobs = await api.runningJobs().catch(() => 0);
      if (stopped) return;
      if (!unsaved && jobs === 0) return void api.restartApp();
      setStage({ name: "waiting", jobs, unsaved });
      timer = setTimeout(() => void look(), POLL_MS);
    };
    void look();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [waiting, hasUnsavedNote]);

  const openPage = (url: string) => {
    setUpdate(null);
    api.openExternal(url).catch((e) => toast(String(e)));
  };

  return (
    <>
      <Button variant="outline" size="sm" disabled={checking} onClick={() => void check()}>
        {checking ? t("Checking…") : t("Check for updates")}
      </Button>
      <Dialog
        open={update !== null}
        // Once installed, the old version must not be left running by closing the dialog.
        onOpenChange={(open) => !open && stage.name === "offer" && setUpdate(null)}
      >
        {update && (
          <DialogContent aria-describedby={undefined} className="flex max-h-[80vh] flex-col gap-4 p-5">
            <DialogTitle>{t("Pergament {version} is available", { version: update.version })}</DialogTitle>
            {/* Escaped by renderMarkdown; links stay text so nothing opens inside the app. */}
            <div
              className="note-md min-h-0 flex-1 overflow-y-auto text-sm"
              dangerouslySetInnerHTML={{ __html: renderMarkdown(update.notes, { links: false }) }}
            />
            {stage.name === "installing" && (
              <div className="space-y-2 text-sm text-muted">
                <p>{t("Downloading the update…")}</p>
                <Progress value={stage.percent} label={t("Downloading the update…")} />
              </div>
            )}
            {stage.name === "waiting" && (stage.unsaved || stage.jobs > 0) && (
              <div className="flex items-center justify-between gap-3 text-sm text-muted">
                <p role="status">
                  {stage.unsaved
                    ? t("Save or close your note to restart.")
                    : t("Waiting for {n} downloads or imports to finish…", { n: stage.jobs })}
                </p>
                {!stage.unsaved && (
                  <Button size="sm" onClick={() => void api.restartApp()}>
                    {t("Restart now")}
                  </Button>
                )}
              </div>
            )}
            {stage.name === "offer" && (
              <div className="flex justify-end gap-2">
                <Button variant="ghost" onClick={() => setUpdate(null)}>
                  {t("Later")}
                </Button>
                {update.installable ? (
                  <Button onClick={() => void install(update.version)}>{t("Update and restart")}</Button>
                ) : (
                  <Button onClick={() => openPage(update.url)}>{t("Open download page")}</Button>
                )}
              </div>
            )}
          </DialogContent>
        )}
      </Dialog>
    </>
  );
}
