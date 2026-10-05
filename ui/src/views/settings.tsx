import { AppBar, useApp } from "@/app";
import type { Theme } from "@/lib/settings";
import { cn } from "@/lib/utils";

const themes: { value: Theme; label: string }[] = [
  { value: "system", label: "Follow system" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function SettingsView() {
  const { theme, setTheme, fontScale, setFontScale } = useApp();
  return (
    <>
      <AppBar title="Settings" />
      <div className="flex-1 overflow-y-auto px-8 py-8">
        <div className="max-w-2xl space-y-10">
          <section>
            <h2 className="mb-3 text-lg font-bold uppercase tracking-wide">Appearance</h2>
            <div className="inline-flex overflow-hidden rounded ring-1 ring-line">
              {themes.map((t) => (
                <button
                  key={t.value}
                  onClick={() => setTheme(t.value)}
                  className={cn(
                    "px-4 py-2 text-sm",
                    theme === t.value ? "bg-brand text-brand-fg" : "bg-surface hover:bg-bar",
                  )}
                >
                  {t.label}
                </button>
              ))}
            </div>
          </section>
          <section>
            <h2 className="mb-3 text-lg font-bold uppercase tracking-wide">Text size</h2>
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
          <section className="text-sm text-muted">
            <h2 className="mb-3 text-lg font-bold uppercase tracking-wide text-fg">About</h2>
            <p>
              jwlinux is an unofficial reader for publications you import or download yourself, for personal use.
              It is not affiliated with Jehovah's Witnesses or the Watch Tower Bible and Tract Society and contains
              no publication content. No telemetry.
            </p>
          </section>
        </div>
      </div>
    </>
  );
}
