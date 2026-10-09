# Translating Pergament

The interface is available in English and German. Adding another language is
one new file and one line in a list. You do not need to know TypeScript; the
file is a list of English texts with room for yours next to them.

This is about the **interface**. The language of publications is a separate
choice (the language menu in the top right corner), because the texts of a
publication come from jw.org.

## How it works

Every text in the app is written in English where it is used, for example
`t("Save")`. A language file maps those English texts to its own:

```ts
export const fr: Record<string, string> = {
  "Save": "Enregistrer",
  "Imported {title}": "{title} importé",
};
```

- An empty text (`""`) or a missing one stays English until someone translates
  it, so a half-finished translation works.
- `{title}`, `{count}` and similar markers are filled in by the app. Keep them
  exactly as they are, but move them wherever your language needs them.
- A few texts begin or end with a space (` from {device}`, because the app
  puts them after another text). Keep that space.

## Add a language

You need [bun](https://bun.sh) (`nix develop` in the repository has it).

1. Make your branch and start the language, with its code, its own name and a
   [language tag](https://www.w3.org/International/articles/language-tags/):

   ```sh
   git switch -c translate-fr
   bun run i18n:new fr "Français" fr-FR
   ```

   This creates `ui/src/locales/fr.ts` with every text still to translate and
   adds the language to `ui/src/locales/index.ts`.

2. Open `ui/src/locales/fr.ts` and fill in the empty texts. They are grouped
   by screen, in the order they appear in the app. Texts you leave empty stay
   English.

3. Check your work as often as you like:

   ```sh
   bun run i18n                # how much is translated, and what is wrong
   bun run i18n -- --missing   # also lists every text still to do
   bun run test --run          # the checks every language must pass
   ```

   It fails on changed `{markers}`, a duplicate code or a language tag the
   browser does not know. Missing texts are not an error.

4. Look at it in the app: `bun run tauri dev`, then Settings, then the
   interface language. Long texts can be too wide for a button; shorten them if
   they are.

5. Open a pull request. Changes to `main` come in through pull requests, so
   please do not push there directly. One language per pull request, with a
   title such as `feat: add French translation`.

Without bun you can do the same by hand: copy `ui/src/locales/de.ts` to
`ui/src/locales/fr.ts`, translate the values, rename the exported object, add
an import and a line to `ui/src/locales/index.ts`, and run the tests.

## Improving a language

Edit its file in `ui/src/locales/` and open a pull request. If a text sounds
wrong only in one place, say where you saw it; the same English text is shared
by every place that uses it.

## New texts in the app

When a change adds a text, wrap it in `t("…")` and add its translation to
`ui/src/locales/de.ts`. Other languages fall back to English until they catch
up: `bun run i18n` shows what each one is missing.

## Limits

- Dates and week ranges follow the language tag. English and German have
  hand-written week ranges; any other language uses the browser's own format.
- Relative times ("3 days ago") have one singular and one plural text. A
  language with more plural forms needs a change in the code first; please
  open an issue.
- Right-to-left languages are not supported yet.
