# The .jwpub format

What jwlinux knows about `.jwpub` files, where each fact comes from, and how
sure we are. Everything under "Verified" was checked against real files;
everything under "Unknown" is explicitly not relied upon.

Fixtures used for verification (not committed, user-supplied):

| File | Symbol | Type | MEPS lang | Notes |
|---|---|---|---|---|
| `nwtsty_X.jwpub` | `nwtsty` | Bible (`type` 5) | 2 (`X`, German) | study Bible, 126 MB |
| `wp_X_202609.jwpub` | `wp26` | Watchtower (`type` 3) | 2 (`X`, German) | public edition no. 1/2026 |

Sources:

- **[jwapi]** MrCyjaneK/jwapi, `docs/jwpub/index.md` (commit `738c15f`)
- **[msp]** sws2apps/meeting-schedules-parser (MIT), `src/common/jwpub_parser.ts` (commit `db0da7a`)
- **[own]** own inspection of the fixtures with `unzip`, `sqlite3`, `sha1sum`, `sha256sum`, `openssl`

## Verified

### Outer container

- A regular ZIP (deflate). The archive comment is `JASPER`. [own]
- Exactly two entries: `manifest.json` and `contents`. [jwapi, own]

### manifest.json

Selected fields (full examples are in [jwapi]):

| Field | Meaning | Source |
|---|---|---|
| `name` | file name, `<symbol>_<langcode>[_<issue>].jwpub` | [jwapi, own] |
| `hash` | **SHA-256 of the raw `contents` entry bytes** (hex) | [jwapi], verified [own] on both fixtures |
| `expandedSize` | sum of the uncompressed sizes of the entries in `contents` | [own] both fixtures |
| `contentFormat` | `"z-a"` in all samples | [jwapi, own] |
| `mepsPlatformVersion` | **number (`2.100000`) in nwtsty, string (`"2.10"`) in wp26**, so the parser must accept both | [own] |
| `publication.fileName` | name of the SQLite DB inside `contents` | [own] |
| `publication.hash` | **SHA-1 of the extracted `.db` file** (hex) | [own] both fixtures |
| `publication.symbol`, `.language`, `.year`, `.issueId`, `.title`, `.publicationType`, `.type` | publication metadata; `language` is the MEPS language index (2 = German) | [own] |
| `publication.images[]` | cover/thumbnail images with `fileName`, `type`, `width`, `height` | [own] |

### contents

- A ZIP using the **store** method (no compression). [own]
- Holds the SQLite DB (`publication.fileName`) and JPEG images referenced by
  `Multimedia.FilePath` (721 files in nwtsty, 29 in wp26). [own]

### SQLite schema

`schemaVersion` 9 in both fixtures. Both share the common tables; the Bible
additionally has `BiblePublication`, `BibleBook`, `BibleChapter`,
`BibleChapterParagraph`, `BibleVerse`, `BibleVerseRanking`,
`BibleMarginalSymbol` and `BibleOutlineEntry`. [own]

Key tables:

| Table | Purpose |
|---|---|
| `Publication` | one row: `MepsLanguageIndex`, `Symbol`, `Year`, `IssueTagNumber`, titles. Used for key derivation |
| `Document` | one row per document; `Content` is encrypted HTML (see below), `ContentLength` = decoded length in bytes; `Class` = document kind |
| `DocumentParagraph` | `ParagraphIndex`, `BeginPosition`/`EndPosition` byte offsets into the decoded content |
| `PublicationViewItem` (+`…Document`, `…Field`) | table of contents tree: `ParentPublicationViewItemId` (-1 = root), `Title`, `DefaultDocumentId` |
| `Multimedia` / `DocumentMultimedia` | images: `FilePath` (file in `contents`), `MimeType`, `Width`, `Height`, `Label` (alt text), encrypted `CaptionContent`, `CreditLineContent` |
| `Footnote` | `DocumentId`, `FootnoteIndex`, encrypted `Content`, `BibleVerseId` |
| `BibleCitation` | cross references: `DocumentId`, `BlockNumber`, `ElementNumber`, `FirstBibleVerseId`/`LastBibleVerseId`, `BibleVerseId` (the verse the reference is attached to) |
| `Extract` / `DocumentExtract` | quoted excerpts from other publications; `Link` like `p/X:502016128/`, `Caption` (plain HTML), encrypted `Content` |
| `InternalLink`, `Hyperlink` (+`Document…` maps) | link targets, `Link` like `p/X:1001070005/` |
| `BibleBook` | 66 rows; `BookDocumentId`, `IntroDocumentId`, `OverviewDocumentId`, `FirstVerseId`/`LastVerseId`, encrypted `Profile`, `BookDisplayTitle` |
| `BibleChapter` | 1189 rows; `BookNumber`, `ChapterNumber`, encrypted `Content` (+ optional `PreContent`, `PostContent`), verse/footnote/citation ID ranges |
| `BibleVerse` | 31194 rows, **IDs start at 0** (Gen 1:1 = 0, Rev 22:21 = 31193); `Label` (plain HTML), encrypted `Content` |
| `VerseCommentary` / `VerseCommentaryMap` | study notes per verse; `Label` (plain HTML with verse link), encrypted `Content` |
| `BiblePublication` | `BibleVersion` = `NWTR` (used in Bible links) |
| `Word`, `SearchIndex*` | search index; `Word` holds lower-cased plain words |

Plain-text (not encrypted) columns include all `Title*` columns,
`BibleVerse.Label`, `Multimedia.Label`/`Caption`, `Extract.Caption`,
`VerseCommentary.Label` and `PublicationViewItem.Title`.

Observed `Document.Class` values in nwtsty: `1` Bible book (66, `Content` is
NULL; the text lives in `BibleChapter`), `11`/`12`/`14` intro and appendix
articles, `113` book introduction, `114`/`115` book overview, `118` study
notes per chapter, `121` Bible navigation, `122` video intro, `125` appendix,
`18` title page, `39` cover. In wp26: `39` cover, `94` introduction, `68`
table of contents, `6` articles. [own]

### Content encoding

All of `Document.Content`, `BibleChapter.Content`/`PreContent`/`PostContent`,
`BibleVerse.Content`, `Footnote.Content`, `Extract.Content`,
`VerseCommentary.Content`, `Multimedia.CaptionContent`/`CreditLineContent`
and `BibleBook.Profile` use the same scheme. The scheme is from [msp]; the
`IssueTagNumber == 0` rule is [own].

1. Build the *publication card* from the `Publication` row:
   `"{MepsLanguageIndex}_{Symbol}_{Year}"`, and if `IssueTagNumber` is not
   `0`/empty append `"_{IssueTagNumber}"`.
   - wp26: `2_wp26_2026_20260900`
   - nwtsty: `2_nwtsty_2025` (**not** `2_nwtsty_2025_0`. [msp] always appends
     the tag, which fails for nwtsty; verified [own])
2. `h = SHA-256(card)` (one round, UTF-8).
3. `k = h XOR C`, where `C` is the fixed 32-byte constant in [msp]
   (`getPubKeyIv`).
4. AES-128-CBC with key `k[0..16]` and IV `k[16..32]`, PKCS#7 padding.
5. zlib inflate (RFC 1950 header). The result is UTF-8 HTML.

Verification [own]: every non-empty blob in all of the columns above decrypts and
inflates to valid UTF-8 in both fixtures (nwtsty: 693 documents, 31194
verses, 1189 chapters, 9736 footnotes, 3557 study notes, …), and the decoded
`Document.Content` length equals `ContentLength` for every row.

`BibleVerse.AdjustmentInfo` is **not** encrypted (3–53 bytes, lengths not a
multiple of 16; e.g. `808082`). Its meaning is unknown.

### Decoded HTML

HTML fragments (no `<html>`/`<body>`), with elements carrying
`id="pN" data-pid="N"` paragraph IDs. [own]

- **Bible verses**: in `BibleChapter.Content` they are
  `<span id="v{book}-{chapter}-{verse}-{part}" class="v">`; in
  `BibleVerse.Content` the suffix is missing: `id="v{book}-{chapter}-{verse}"`.
  **Psalm superscriptions are verse 0** (e.g. `v19-3-0`); their
  `BibleVerse.Label` is empty (116 rows). The first verse of a chapter is
  labelled `<span class="cl">{chapter}</span>`, the others
  `<span class="vl">{verse}</span>`. Mapping every `BibleVerseId` to
  book/chapter (via `BibleChapter.FirstVerseId..LastVerseId`) and verse (via
  `Label`) agrees with the span id for all 31194 verses (test
  `study_bible_verse_refs`).
  Verse numbers are `<span class="vl">`; chapter numbers are
  `<span class="cl"><strong>1</strong>`. Empty tooltip helper spans have
  class `tt`.
- **Footnote markers**: `<span data-fnid="N" class="fn">a…</span>`. `N` is
  `Footnote.FootnoteIndex` within the same `Footnote.DocumentId`. For Bible
  chapters that is the book's `BookDocumentId`; the index runs on across the
  chapters of a book (Gen 1: 1–11, Gen 2: 12–27, matching
  `BibleChapter.FirstFootnoteId..LastFootnoteId`). Footnote content is
  `<div id="footnoteN" data-fnid="N" class="fcc fn-ref"><p>…</p></div>`.
  Regular documents (e.g. Watchtower articles) already contain their
  footnotes inline in a `<div class="groupFootnote">` with the same
  `id="footnoteN"` and a back link `href="#footnotesourceN"`; Bible chapters
  only carry the markers.
- **Marginal cross-reference markers**: `<span data-mid="N" class="m">a…</span>`.
  `N` is `BibleCitation.BlockNumber` within the book document; the rows with
  that block (ordered by `ElementNumber`) are the referenced verse ranges.
  Gen 2's first marker `mid=35` matches the chapter's `FirstBibleCitationId`
  row.
- **Bible links**: `<a href="jwpub://b/NWTR/{book}:{ch}:{v}-{book}:{ch}:{v}" class="b">`.
- **Publication links**: `<a href="jwpub://p/X:{MepsDocumentId}/" class="xt" data-xtid="N">`,
  where `X` is the MEPS language code. Some carry a suffix after the slash,
  e.g. `jwpub://p/X:1001070144/1-1` in study notes (meaning unknown).
- **Study notes** (`VerseCommentary.Content`) are the same text as the
  per-chapter study note documents (`Document.Class` 118), split per verse;
  they exist from Matthew 1:1 (`BibleVerseId` 23261) on.
- **External links**: `https://www.jw.org/finder?wtlocale=X&docid=…`.
- **Images**: `<figure><img src="jwpub-media://{Multimedia.FilePath}" alt=… width=… height=…>`.
  The file is inside `contents`.
- Styling classes are site CSS (`du-*`, `dc-*`), which is not part of the file.

## Unknown / not relied upon

- What the `C` constant protects beyond obfuscation. We only reproduce the
  documented algorithm.
- `BibleVerse.AdjustmentInfo` meaning.
- Exact semantics of `data-xtid` (likely the per-document extract number;
  not confirmed), `data-bid` (looks like `{block}-{element}`) and
  `data-vlid`.
- `BibleCitation.MarginalClassification`, `SortPosition`.
- The suffix in publication links like `jwpub://p/X:1001070144/1-1`.
- `VerseMultimediaMap` (media attached to verses) is not rendered yet.
- Abbreviated Bible book names: no table holding them was found;
  cross references use `BibleBook.ChapterDisplayTitle` (e.g. "Psalm").
- Full list of `Document.Class`/`Type` values across other publication types.
- `contentFormat` values other than `z-a`.
- Whether older `schemaVersion`s (e.g. 8 in [jwapi]) differ in a way that matters.
- Download API endpoints (not investigated yet).
