import { de } from "@/locales/de";
import { ptBR } from "@/locales/pt-BR";
import { fr } from "@/locales/fr";
import { it } from "@/locales/it";

/** An interface language. English is the source language and has no entry here. */
export interface Locale {
  /** Short code, also the file name: `de`, `fr`, `pt-BR`, `it`. */
  code: string;
  /** The language's own name, as shown in Settings. */
  name: string;
  /** BCP 47 tag for dates and numbers: `de-DE`, `fr-FR`, `it-IT`. */
  tag: string;
  /** Texts keyed by the English text; a missing or empty one stays English. */
  texts: Record<string, string>;
}

/** Every translation. `bun run i18n:new` adds a line here; see docs/TRANSLATING.md. */
export const locales: Locale[] = [
  { code: "de", name: "Deutsch", tag: "de-DE", texts: de },
  { code: "pt-BR", name: "Português (Brasil)", tag: "pt-BR", texts: ptBR },
  { code: "fr", name: "Français", tag: "fr-FR", texts: fr },
  { code: "it", name: "Italiano", tag: "it-IT", texts: it },
];
