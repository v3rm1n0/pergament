import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { AppUpdate } from "@/components/app-update";
import { t } from "@/lib/i18n";

export function AboutTab() {
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion, () => undefined);
  }, []);
  return (
    <div className="space-y-10">
      <section className="text-sm text-muted">
        <h2 className="mb-3 text-sm font-semibold text-fg">{t("About")}</h2>
        {version && <p className="mb-2">{t("Version {version}", { version })}</p>}
        <div className="mb-4">
          <AppUpdate />
        </div>
        <p>
          {t(
            "Pergament is an unofficial reader for publications you import or download yourself, for personal use. It is not affiliated with Jehovah's Witnesses or the Watch Tower Bible and Tract Society and contains no publication content. No telemetry.",
          )}
        </p>
      </section>
    </div>
  );
}
