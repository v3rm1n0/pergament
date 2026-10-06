import { AppBar, useApp } from "@/app";
import type { Theme } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import type { UiLangSetting } from "@/lib/i18n";

const uiLangs: { value: UiLangSetting; label: string }[] = [
  { value: "system", label: "Follow system" },
  { value: "en", label: "English" },
  { value: "de", label: "Deutsch" },
];

const themes: { value: Theme; label: string }[] = [
  { value: "system", label: "Follow system" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function SettingsView() {
  const { uiLang, setUiLang, theme, setTheme, fontScale, setFontScale, createBackup, restoreBackup } = useApp();
  return (
    <>
      <AppBar title={t("Settings")} />
      <div className="flex-1 overflow-y-auto px-8 py-8">
        <div className="max-w-2xl space-y-10">
          <section>
            <h2 className="mb-3 text-lg font-bold uppercase tracking-wide">{t("Interface language")}</h2>
            <div className="inline-flex overflow-hidden rounded ring-1 ring-line">
              {uiLangs.map((l) => (
                <button
                  key={l.value}
                  onClick={() => setUiLang(l.value)}
                  className={cn(
                    "px-4 py-2 text-sm",
                    uiLang === l.value ? "bg-brand text-brand-fg" : "bg-surface hover:bg-bar",
                  )}
                >
                  {t(l.label)}
                </button>
              ))}
            </div>
          </section>
          <section>
            <h2 className="mb-3 text-lg font-bold uppercase tracking-wide">{t("Appearance")}</h2>
            <div className="inline-flex overflow-hidden rounded ring-1 ring-line">
              {themes.map((th) => (
                <button
                  key={th.value}
                  onClick={() => setTheme(th.value)}
                  className={cn(
                    "px-4 py-2 text-sm",
                    theme === th.value ? "bg-brand text-brand-fg" : "bg-surface hover:bg-bar",
                  )}
                >
                  {t(th.label)}
                </button>
              ))}
            </div>
          </section>
          <section>
            <h2 className="mb-3 text-lg font-bold uppercase tracking-wide">{t("Text size")}</h2>
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
            <p className="reader mt-4 max-w-xl text-fg">Am Anfang erschuf Gott Himmel und Erde.</p>
          </section>
          <section>
            <h2 className="mb-3 text-lg font-bold uppercase tracking-wide">{t("Backup")}</h2>
            <div className="flex flex-wrap gap-2">
              <button
                onClick={() => void createBackup()}
                className="bg-brand px-4 py-2 text-sm text-brand-fg hover:brightness-110"
              >
                {t("Create backup")}
              </button>
              <button onClick={() => void restoreBackup()} className="bg-tile px-4 py-2 text-sm hover:brightness-110">
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
            <h2 className="mb-3 text-lg font-bold uppercase tracking-wide text-fg">{t("About")}</h2>
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
