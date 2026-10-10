import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { AppBar, useApp } from "@/app";
import type { Theme } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { t, uiLanguages } from "@/lib/i18n";
import type { UiLangSetting } from "@/lib/i18n";

const uiLangs: { value: UiLangSetting; label: string }[] = [
  { value: "system", label: "Follow system" },
  ...uiLanguages().map((l) => ({ value: l.code, label: l.name })),
];

const themes: { value: Theme; label: string }[] = [
  { value: "system", label: "Follow system" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function SettingsView() {
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion, () => undefined);
  }, []);
  const { uiLang, setUiLang, theme, setTheme, fontScale, setFontScale, createBackup, restoreBackup, display, updates } = useApp();
  return (
    <>
      <AppBar title={t("Settings")} />
      <div className="page flex-1 overflow-y-auto">
        <div className="space-y-10">
          <section>
            <h2 className="mb-3 text-sm font-semibold">{t("Interface language")}</h2>
            <div className="inline-flex overflow-hidden rounded-md border border-line">
              {uiLangs.map((l) => (
                <button
                  key={l.value}
                  onClick={() => setUiLang(l.value)}
                  aria-pressed={uiLang === l.value}
                  className={cn(
                    "px-4 py-2 text-sm",
                    uiLang === l.value ? "bg-bar font-medium" : "bg-surface text-muted hover:bg-bar hover:text-fg",
                  )}
                >
                  {l.value === "system" ? t(l.label) : l.label}
                </button>
              ))}
            </div>
          </section>
          <section>
            <h2 className="mb-3 text-sm font-semibold">{t("Appearance")}</h2>
            <div className="inline-flex overflow-hidden rounded-md border border-line">
              {themes.map((th) => (
                <button
                  key={th.value}
                  onClick={() => setTheme(th.value)}
                  aria-pressed={theme === th.value}
                  className={cn(
                    "px-4 py-2 text-sm",
                    theme === th.value ? "bg-bar font-medium" : "bg-surface text-muted hover:bg-bar hover:text-fg",
                  )}
                >
                  {t(th.label)}
                </button>
              ))}
            </div>
          </section>
          <section>
            <h2 className="mb-3 text-sm font-semibold">{t("Text size")}</h2>
            <div className="flex items-center gap-4">
              <input
                type="range"
                min={0.8}
                max={1.6}
                step={0.05}
                value={fontScale}
                onChange={(e) => setFontScale(Number(e.target.value))}
                className="w-72 accent-[var(--accent)]"
              />
              <span className="w-12 text-sm text-muted">{Math.round(fontScale * 100)} %</span>
            </div>
            <p className="reader mt-4 max-w-xl text-fg">{t("In the beginning God created the heavens and the earth.")}</p>
          </section>
          <section>
            <h2 className="mb-3 text-sm font-semibold">{t("Updates")}</h2>
            <label className="flex cursor-pointer items-start gap-3">
              <input
                type="checkbox"
                checked={updates.enabled}
                onChange={(e) => updates.setEnabled(e.target.checked)}
                className="mt-1 h-4 w-4 accent-[var(--accent)]"
              />
              <span>{t("Check for publication updates at startup")}</span>
            </label>
            <p className="mt-2 max-w-xl text-sm text-muted">
              {t(
                "Compares your downloaded publications with the catalog, which is refreshed at most once a day, and only if you have loaded the catalog before. Nothing is downloaded until you update.",
              )}
            </p>
          </section>
          <section>
            <h2 className="mb-3 text-sm font-semibold">{t("Second display")}</h2>
            <label className="flex cursor-pointer items-start gap-3">
              <input
                type="checkbox"
                checked={display.enabled}
                onChange={(e) => void display.setEnabled(e.target.checked)}
                className="mt-1 h-4 w-4 accent-[var(--accent)]"
              />
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
              <button
                onClick={() => void createBackup()}
                className="rounded-md bg-brand px-4 py-2 text-sm text-brand-fg hover:opacity-90 dark:bg-fg dark:text-surface"
              >
                {t("Create backup")}
              </button>
              <button onClick={() => void restoreBackup()} className="rounded-md border border-line bg-surface px-4 py-2 text-sm hover:bg-bar">
                {t("Restore backup…")}
              </button>
            </div>
            <p className="mt-2 text-sm text-muted">
              {t(
                "Highlights, notes, tags and bookmarks as a .jwlibrary file. Restoring replaces the current data; the previous data is kept as userData.db.before-restore in the library folder.",
              )}
            </p>
          </section>
          <section className="text-sm text-muted">
            <h2 className="mb-3 text-sm font-semibold text-fg">{t("About")}</h2>
            {version && <p className="mb-2">{t("Version {version}", { version })}</p>}
            <p>
              {t(
                "Pergament is an unofficial reader for publications you import or download yourself, for personal use. It is not affiliated with Jehovah's Witnesses or the Watch Tower Bible and Tract Society and contains no publication content. No telemetry.",
              )}
            </p>
          </section>
        </div>
      </div>
    </>
  );
}
