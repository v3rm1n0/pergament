import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ask, message as messageDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
import { useApp } from "@/app";
import { NetworkBadge } from "@/components/network-badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { api, type CacheUsage } from "@/lib/api";
import { formatSize, rows } from "@/lib/cache";
import { t, locale } from "@/lib/i18n";
import { conflictText } from "@/lib/merge";
import { syncErrorText } from "@/lib/sync";
import type { RollingSettings, SyncStatus } from "@/lib/types";

function CacheSection() {
  const { toast, downloads } = useApp();
  const [usage, setUsage] = useState<CacheUsage | null>(null);
  const [failed, setFailed] = useState(false);
  const refresh = useCallback(
    () =>
      api.cacheUsage().then(
        (u) => {
          setUsage(u);
          setFailed(false);
        },
        () => {
          setUsage(null);
          setFailed(true);
        },
      ),
    [],
  );
  // Clearing removes the partial files of running downloads, so wait for them.
  const downloading = Object.keys(downloads).length > 0;
  useEffect(() => void refresh(), [refresh]);
  const clear = async () => {
    const ok = await ask(
      t(
        "Removes the catalog, cover images and unfinished downloads. Downloaded publications and your notes stay. The catalog (about 58 MB) is downloaded again when you next need it.",
      ),
      { title: t("Clear cache?"), kind: "warning", okLabel: t("Clear cache") },
    );
    if (!ok) return;
    try {
      await api.clearCache();
      toast(t("Cache cleared"));
    } catch (e) {
      toast(t("Could not clear the cache: {error}", { error: String(e) }));
    }
    await refresh();
  };
  return (
    <section>
      <h2 className="mb-3 text-sm font-semibold">{t("Cache")}</h2>
      {usage && (
        <dl className="mb-3 max-w-xs space-y-1 text-sm">
          {rows(usage).map((r) => (
            <div key={r.key} className="flex justify-between gap-4">
              <dt className="text-muted">{r.label}</dt>
              <dd>{formatSize(r.bytes)}</dd>
            </div>
          ))}
          <div className="flex justify-between gap-4 border-t border-line pt-1 font-medium">
            <dt>{t("Total")}</dt>
            <dd>{formatSize(usage.total)}</dd>
          </div>
        </dl>
      )}
      <Button variant="outline" onClick={() => void clear()} disabled={!usage || usage.total === 0 || downloading}>
        {t("Clear cache")}
      </Button>
      {failed && <p className="mt-2 text-sm text-muted">{t("Cannot measure the cache")}</p>}
      {downloading && <p className="mt-2 text-sm text-muted">{t("Wait for the running downloads to finish.")}</p>}
      <p className="mt-2 max-w-xl text-sm text-muted">
        {t("Cached files are downloaded again when you need them.")}
        <NetworkBadge />
      </p>
    </section>
  );
}

function lastSynced(s: SyncStatus): string {
  const time = new Date((s.lastSync ?? 0) * 1000).toLocaleString(locale(), { dateStyle: "medium", timeStyle: "short" });
  return s.lastDevice ? t("Last synced {time} with {device}", { time, device: s.lastDevice }) : t("Last synced {time}", { time });
}

function AutoBackupSection() {
  const { toast } = useApp();
  const [rolling, setRolling] = useState<RollingSettings | null>(null);
  const [draft, setDraft] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  useEffect(() => {
    api.autoSettings().then(
      (s) => {
        setRolling(s.rolling);
        setFolder(s.sync.folder);
      },
      (e) => setLoadError(String(e)),
    );
  }, []);
  const [folder, setFolder] = useState<string | null>(null);
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const refreshStatus = useCallback(
    () =>
      api.syncStatus().then(
        (s) => {
          setStatus(s);
          setSyncError(null);
          return s;
        },
        (e) => {
          setSyncError(String(e));
          return null;
        },
      ),
    [],
  );
  useEffect(() => {
    void refreshStatus();
    const timer = setInterval(() => void refreshStatus(), 30_000);
    const changed = listen("user-data-changed", () => void refreshStatus());
    return () => {
      clearInterval(timer);
      void changed.then((f) => f());
    };
  }, [refreshStatus]);
  /** Runs a sync command: shows its error, keeps the buttons from firing twice, and takes in the new status. */
  const run = async (action: () => Promise<SyncStatus | void>) => {
    setBusy(true);
    setSyncError(null);
    try {
      const s = await action();
      if (s) setStatus(s);
      else await refreshStatus();
    } catch (e) {
      setSyncError(syncErrorText(String(e)));
    } finally {
      setBusy(false);
    }
  };
  const choose = () =>
    void run(async () => {
      const picked = await openDialog({ directory: true, multiple: false });
      const path = Array.isArray(picked) ? picked[0] : picked;
      if (!path) return;
      setFolder((await api.setSyncFolder(path)).sync.folder);
      // An unwritable folder is only reported by the status.
      return (await api.syncStatus()) ?? undefined;
    });
  const stop = () =>
    void run(async () => {
      setFolder((await api.setSyncFolder(null)).sync.folder);
      return api.syncStatus();
    });
  const showConflicts = async () => {
    const text = conflictText(status?.conflicts ?? []);
    try {
      await messageDialog(text, { title: t("Conflicts"), kind: "warning" });
    } catch {
      toast(text);
    }
  };
  const syncFolder = status?.folder ?? folder;
  const statusLine = syncError ?? status?.error ?? (status?.lastSync ? lastSynced(status) : null);
  const save = async (enabled: boolean, keep: number) => {
    try {
      setRolling((await api.setRolling(enabled, keep)).rolling);
    } catch (e) {
      toast(String(e));
    }
  };
  const commit = () => {
    const n = Number(draft);
    setDraft(null);
    if (rolling && draft !== null && draft.trim() !== "" && Number.isInteger(n) && n >= 1 && n <= 100 && n !== rolling.keep)
      void save(rolling.enabled, n);
  };
  return (
    <section>
      <h2 className="mb-3 text-sm font-semibold">{t("Automatic backups and sync")}</h2>
      <label className="flex cursor-pointer items-start gap-3">
        <Switch
          checked={rolling?.enabled ?? false}
          disabled={!rolling}
          onCheckedChange={(on) => rolling && void save(on, rolling.keep)}
          className="mt-0.5"
        />
        <span>{t("Write backups automatically")}</span>
      </label>
      <label className="mt-3 flex items-center gap-2 text-sm">
        <span>{t("Keep the last … backups")}</span>
        <Input
          type="number"
          min={1}
          max={100}
          className="w-20"
          disabled={!rolling}
          value={draft ?? rolling?.keep ?? ""}
          aria-label={t("Keep the last … backups")}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === "Enter" && commit()}
        />
      </label>
      <Button variant="outline" className="mt-3" onClick={() => void api.openBackupsFolder().catch((e) => toast(String(e)))}>
        {t("Open backups folder")}
      </Button>
      <div className="mt-6">
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="outline" onClick={choose} disabled={busy}>
            {t("Choose sync folder…")}
          </Button>
          {syncFolder && (
            <>
              <Button variant="outline" onClick={() => void run(api.syncNow)} disabled={busy}>
                {t("Sync now")}
              </Button>
              <Button variant="outline" onClick={stop} disabled={busy}>
                {t("Stop syncing")}
              </Button>
            </>
          )}
        </div>
        {syncFolder && <p className="mt-2 break-all text-sm">{syncFolder}</p>}
        {statusLine && <p className="mt-1 text-sm text-muted">{statusLine}</p>}
        {status?.pending.map((p) => (
          <div key={p.file} className="mt-2 text-sm">
            <p>{t("{device} would delete {n} items.", { device: p.device, n: p.deletions })}</p>
            <div className="mt-1 flex gap-2">
              <Button variant="outline" onClick={() => void run(() => api.resolvePeer(p.file, true))} disabled={busy}>
                {t("Apply deletions")}
              </Button>
              <Button variant="outline" onClick={() => void run(() => api.resolvePeer(p.file, false))} disabled={busy}>
                {t("Keep my data")}
              </Button>
            </div>
          </div>
        ))}
        {status && status.conflicts.length > 0 && (
          <Button variant="outline" className="mt-2" onClick={() => void showConflicts()}>
            {t("Show conflicts")}
          </Button>
        )}
        <p className="mt-2 max-w-xl text-sm text-muted">
          {t(
            "Choose a folder that Nextcloud, Syncthing, Dropbox or another sync app keeps in sync. Each computer writes its own file there and merges the others; deletions carry over.",
          )}
        </p>
      </div>
      {loadError && (
        <p className="mt-2 text-sm text-muted">{t("Could not load the backup settings: {error}", { error: loadError })}</p>
      )}
      <p className="mt-2 max-w-xl text-sm text-muted">
        {t(
          "A backup is written to the backups folder in the library folder when you close Pergament and once a day, if something changed. Older ones are deleted.",
        )}
      </p>
    </section>
  );
}

export function DataTab() {
  const { createBackup, restoreBackup, mergeBackups, setMergeBackups, display, updates } = useApp();
  return (
    <div className="space-y-10">
      <section>
        <h2 className="mb-3 text-sm font-semibold">{t("Updates")}</h2>
        <label className="flex cursor-pointer items-start gap-3">
          <Switch checked={updates.enabled} onCheckedChange={updates.setEnabled} className="mt-0.5" />
          <span>{t("Check for publication updates at startup")}</span>
        </label>
        <p className="mt-2 max-w-xl text-sm text-muted">
          {t(
            "Compares your downloaded publications with the catalog, which is refreshed at most once a day, and only if you have loaded the catalog before. Nothing is downloaded until you update.",
          )}
          <NetworkBadge />
        </p>
      </section>
      <section>
        <h2 className="mb-3 text-sm font-semibold">{t("Second display")}</h2>
        <label className="flex cursor-pointer items-start gap-3">
          <Switch checked={display.enabled} onCheckedChange={(on) => void display.setEnabled(on)} className="mt-0.5" />
          <span>{t("Use a second display")}</span>
        </label>
        <p className="mt-2 max-w-xl text-sm text-muted">
          {t(
            "Opens a black window, fullscreen on another monitor if there is one, that shows the year text. Recordings and pictures you open are shown there too, without controls. If the window opens on the wrong screen, move it there and press F to switch to fullscreen.",
          )}
        </p>
      </section>
      <section>
        <h2 className="mb-3 text-sm font-semibold">{t("Backup")}</h2>
        <div className="flex flex-wrap gap-2">
          <Button onClick={() => void createBackup()}>{t("Create backup")}</Button>
          <Button variant="outline" onClick={() => void restoreBackup()}>
            {mergeBackups ? t("Merge backup…") : t("Restore backup…")}
          </Button>
        </div>
        <label className="mt-3 flex cursor-pointer items-start gap-3">
          <Switch checked={mergeBackups} onCheckedChange={setMergeBackups} className="mt-0.5" />
          <span>{t("Merge backups instead of replacing")}</span>
        </label>
        <p className="mt-2 max-w-xl text-sm text-muted">
          {t(
            "Favorites, highlights, notes, tags and bookmarks as a .jwlibrary file. Restoring replaces the current data; with merging turned on, backups are merged into it instead and the newer version of everything in both is kept. Either way the previous data is kept as userData.db.before-restore in the library folder.",
          )}
        </p>
      </section>
      <AutoBackupSection />
      <CacheSection />
    </div>
  );
}
