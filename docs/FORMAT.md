# The .jwpub format

What Pergament knows about `.jwpub` files, where each fact comes from, and how
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
| `Extract` / `DocumentExtract` | quoted excerpts from other publications; `Link` like `p/X:502016128/`, `Caption` (plain HTML), encrypted `Content`. A citation anchor (see "Publication links") is taken to carry the same link as `Link` with `p/` in place of the scheme (`jwpub://p/X:502016128/3-5` ↔ `p/X:502016128/3-5`); Pergament matches it exactly, with either trailing slash, or by the head before the range when that head is unique. Inferred from the two shapes, not yet probed in a file; the opt-in test `extracts_resolve_on_real_files` checks it against a user-supplied publication |
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
  row. `MarginalClassification` is per reference: 0 = rows without a
  `BibleVerseId` (not attached to a verse), 1 = ordinary cross reference,
  **2 = parallel account** (Mat 4:1 → Mar 1:12, 13 and Luk 4:1-4), 3 = quotation
  from an earlier book (references in Matthew point into Isaiah, Micah, …). A
  block can mix classes (555 do in the German Study Bible).
- **Bible book intro videos**: a `Multimedia` row with `video/mp4`, a `KeySymbol` (`nwtsv`) and a `Track` (book number × 10: Mat 400) that
  `DocumentMultimedia` ties to the book's document (`BookDocumentId`) or its introduction (`IntroDocumentId`). Other
  videos of those documents are clips in notes with an empty `KeySymbol`. The recording is fetched like any linked one.
- **Bible links**: `<a href="jwpub://b/NWTR/{book}:{ch}:{v}-{book}:{ch}:{v}" class="b">`.
- **Publication links**: `<a href="jwpub://p/X:{MepsDocumentId}/" class="xt" data-xtid="N">`,
  where `X` is the MEPS language code. Some carry a suffix after the slash,
  e.g. `jwpub://p/X:1001070144/1-1` in study notes (meaning unknown). Some
  point at a Bible **book** document (e.g. 1001070144 = Matthew), which has no
  content; Pergament opens chapter 1 of that book.
- **Verse links in book documents**: `jwpub://c/X:{book MepsDocumentId}/{ch}:{v}[-{ch}:{v}]`
  (12,604 in nwtsty). Outlines use them plainly (`…/1:1-1:31`, 5,733 links);
  study notes add `$`-separated alternatives that name the target study
  note's paragraphs, e.g. `jwpub://c/X:1001070145/8:38$p/X:1001070636/47-47:752`
  = Mark 8:38, paragraph 47 of "Markus: Studienanmerkungen zu Kapitel 8",
  which is exactly `VerseCommentary` 757 (`CommentaryMepsDocumentId`
  1001070636, `BeginParagraphOrdinal` 47). The meaning of `:752` is unknown.
  6 links have a bare chapter before the dash (`16-17:5`).
- **Book abbreviations** ("1Mo", "Jos", "Apg"): no table holds them. They are
  the labels of Bible links (`<a href="jwpub://b/NWTR/1:5:1…">1Mo 5:1</a>`);
  taking the most common label word per book gives one clear abbreviation for
  all 66 books in nwtsty, matching the original app's study pane.
- **Outline** (`BibleOutlineEntry`): `Level`, `BeginChapterNumber`/`BeginVerseNumber`,
  `EndChapterNumber`/`EndVerseNumber`, encrypted `Content` (an `<ul class="outline">`
  fragment; the innermost `<p>` is the line), `Book`, `Class` (115 = the book's
  outline document `BibleBook.OutlineDocumentId`, e.g. "1. Mose: Übersicht";
  114 also occurs). Genesis 12 gives "Abram zieht von Haran nach Kanaan (1-9)",
  "Gott gibt Abram Versprechen (7)", "Abram und Sarai in Ägypten (10-20)", as
  in the original app.
- **Footnotes and cross references per verse**: `Footnote.BibleVerseId` and
  `BibleCitation.BibleVerseId` give the verse they belong to.
- **Language code of a file**: the manifest `name` is `{symbol}_{code}[_{issue}].jwpub`.
- **Study notes** (`VerseCommentary.Content`) are the same text as the
  per-chapter study note documents (`Document.Class` 118), split per verse;
  they exist from Matthew 1:1 (`BibleVerseId` 23261) on.
- **External links**: `https://www.jw.org/finder?wtlocale=X&docid=…`.
- **Year text**: the title page of the daily text booklet (`es26` for 2026, the
  document with `Document.Class` 18) has `<p class="sn">Jahrestext „…“ (<a …>Matthäus 5:3</a>)</p>`.
  The words before the first quotation mark are the label, the last link is the
  reference. [own]
- **Images**: `<figure><img src="jwpub-media://{Multimedia.FilePath}" alt=… width=… height=…>`.
  The file is inside `contents`.
- Styling classes are site CSS (`du-*`, `dc-*`), which is not part of the file.

## Download services

Endpoints come from [jwapi] (`notes.txt`, `libjw/jwhttp/*.go`) and [msp]
(`test/e2e/*.test.js`). All were checked with live requests on 2026-10-05,
sent with Pergament's own User-Agent and no app tokens. jwapi's notes impersonate
the official app (`User-Agent: jwlibrary-android`, bearer tokens from
`tokens/jwl-public.jwt`); Pergament does **not** do that and does not use any
endpoint that needs such credentials.

### Publication catalog

1. `GET https://app.jw-cdn.org/catalogs/publications/v4/manifest.json`
   returns `{"version": 1, "current": "<uuid>"}`. [jwapi, own]
2. `GET https://app.jw-cdn.org/catalogs/publications/v4/<uuid>/catalog.db.gz`
   returns a gzipped SQLite database (58 MB gzipped, 216 MB unpacked on
   2026-10-05). Ranged requests are supported. [jwapi, own]

Relevant tables [own]:

| Table | Notes |
|---|---|
| `Publication` | `Id`, `MepsLanguageId`, `KeySymbol` (undated symbol, e.g. `wp`, `w`, `nwtsty`; never NULL), `Symbol` (dated, e.g. `wp26`), `IssueTagNumber` (0 for undated), `Year`, `Title`, `ShortTitle`, `IssueTitle`, `CoverTitle`, `PublicationTypeId`. Unique on (`KeySymbol`, `IssueTagNumber`, `MepsLanguageId`) |
| `PublicationAsset` | one per publication; `MimeType` is always `application/x-jwpub`; **`Size` = file size, `ExpandedSize` = manifest `expandedSize`, `Signature` = SHA-1 of the `.jwpub` file** (verified on both fixtures) |
| `ImageAsset`, `PublicationAssetImageMap` | cover images; `NameFragment` like `images/2b/302014021_univ_sqr-126.jpg`, served at `https://app.jw-cdn.org/catalogs/publications/{NameFragment}` ([jwapi] notes; fetched once, `image/jpeg`). Kinds: `sqr` (square 120/270/600), `lsr` (wide), `cvr` (cover) |
| `DatedText` | `Class`, `Start`, `End`, `PublicationId`. Class **4** = daily text (`es26`, whole year), **68** = Watchtower study weeks (`w`), **106** = meeting workbook weeks (`mwb`); also 91, 92 before 2010. For 2026-10-07 in German: `w` 2026-08 for 2026-10-05..11-01 and `mwb` 2026-09 for 2026-09-07..11-01, matching the original app's Meetings view |
| `CuratedAsset` | `ListType`, `SortOrder`, `PublicationAssetId`. List **2** = "Teaching Toolbox" on the original Home screen (lff, ll, wp26, tracts T-ftr, T-fam, …), list **0** = "Other Meeting Publications" (sjj, lmd, th, S-38), list 1 = sjj only |
| `PublicationAttribute`(`Map`) | named attributes, e.g. `Convention` (the original's "Convention Releases"), `Study`, `Public`, `Archive` |

`PublicationTypeId` has no name table. Names follow from the symbols per type
and match the original app's Library categories: 1 Bible (nwt, nwtsty), 2
Books (bh, cl, lff), 4 Brochures and Booklets (fg, ll, es), 6 Index (rsg
"Studienleitfaden"), 7 Kingdom Ministry (km), 10 Tracts and Invitations (T-…,
CO-inv), 13 Awake! (g), 14 Watchtower (w, wp), 17 Guidelines (S-38), 22 Article
Series (ijw…), 30 Meeting Workbooks (mwb), 31 Programs (CA-…, CO-…).
"What's New" in the original app is the newest `PublicationAsset.CatalogedOn`
(sjj, es27, rsg, gwt, mwb 2027-01 on 2026-10-05).
Type 12 is the 2017 branch circuit assembly program (`CA-brpgm17`) in other languages and is listed with Programs. Attributes split the types further: `Archive` (older publications), `Yearbook`, `Examining the Scriptures`, `Kingdom News`, `Convention`, `Circuit Assembly`, and `Study`/`Public` for the Watchtower from 2008.

**Updates.** A downloaded publication has a newer version when the catalog's
`PublicationAsset.Signature` (SHA-1 of the `.jwpub` file) differs from the
SHA-1 of the file it was imported from, which the library records since it
imports. Older entries only have their unpacked files: their total size equals
`PublicationAsset.ExpandedSize` (17 of 19 installed publications matched
exactly; the others were missing from the catalog or out of date), so a
different size means an update. [own]

The catalog has **no language table**. Pergament derives MEPS-id →
language-code pairs from language-specific image names
(`…_{CODE}_cvr.jpg` etc.), taking the most frequent code per
`MepsLanguageId`. This covers 313 of 875 language ids (0 `E`, 1 `S`, 2 `X`,
3 `F`, 4 `I`, …); every derived code exists in the jw.org language list.
It is a heuristic, not an official mapping. For other languages pass the
MEPS id explicitly.

### Language list

`GET https://www.jw.org/en/languages/` returns JSON
`{"languages": [{"symbol", "langcode", "name", "vernacularName", "direction", "isSignLanguage", …}]}`.
It has no MEPS ids. [jwapi `utils/getmepslangs`, own]

### Download links (pub-media)

`GET https://b.jw-cdn.org/apis/pub-media/GETPUBMEDIALINKS?output=json&pub={KeySymbol}&issue={YYYYMM}&fileformat=JWPUB&alllangs=0&langwritten={CODE}`
([msp] uses `b.jw-cdn.org`, [jwapi] `app.jw-cdn.org`). Omit `issue` for undated
publications. Both `issue=202609` and `issue=20260900` work; the response
reports `"issue": "202609"`. [own]

Response (fields used):
`files.{CODE}.JWPUB[]` → `{ title, filesize, file: { url, checksum, modifiedDatetime } }`.
**`file.checksum` is the MD5 of the `.jwpub` file**, and `filesize` its size
(both verified on both fixtures). File URLs are on `https://cfp2.jw-cdn.org/…`.
They support `Range` requests (`206` with `content-range`, checked byte for
byte). An unknown publication returns HTTP `400` with
`[{"id": …, "title": "Bad Request", "status": 400}]`. [own]

### Recordings (pub-media MP4 and MP3)

Videos and audio are not inside a `.jwpub`. Content links to them as
`https://www.jw.org/finder?lank=pub-{KeySymbol}[_{YYYYMM}]_{track}_{VIDEO|AUDIO}&wtlocale={CODE}`
(`pub-mwbv_202609_1_VIDEO`, undated `pub-jwb-098_7_VIDEO`). The `Multimedia`
table has matching rows without a `FilePath`: `MimeType` `video/mp4`,
`KeySymbol`, `Track`, `IssueTagNumber`, `MepsLanguageIndex`. [own]

`GET https://b.jw-cdn.org/apis/pub-media/GETPUBMEDIALINKS?output=json&pub={KeySymbol}&track={N}&fileformat=MP4&alllangs=0&langwritten={CODE}[&issue={YYYYMM}]`
(`fileformat=MP3` for audio) returns `files.{CODE}.MP4[]` with one entry per
quality: `title`, `label` (`240p`, `360p`, …), `mimetype`, `filesize`,
`duration` (seconds), `frameHeight`, `file.url` (on `cfp2.jw-cdn.org`),
`trackImage.url` (poster) and `subtitles.url` (WebVTT). Pergament streams
`file.url` in the app and stores nothing; subtitles are not used yet. [own]

### Media catalog (mediator)

The Video and Audio tabs come from the public catalog the website uses; it
needs no token. Checked with live requests on 2026-10-09, German. [own]

`GET https://b.jw-cdn.org/apis/mediator/v1/categories/{CODE}/{key}?detailed={0|1}`

- Root keys: `VideoOnDemand` (14 containers such as `VODBible`, 32 KB with
  `detailed=0`) and `Audio` (13 lists, 1.4 MB, recordings included).
  `detailed=1` on a container adds the recordings of its subcategories
  (`VODBible`: 8 sections, 382 recordings, 1.9 MB). A leaf (`type`
  `ondemand`, e.g. `VODBibleReadingStudy`, `AudioInternationalMusic`) has
  `media[]`. An unknown key returns HTTP 404.
- `category`: `key`, `name`, `type` (`container` or `ondemand`), `images`,
  `subcategories[]`, `media[]`.
- `media[]`: `naturalKey` (`pub-nwtsv_X_1_VIDEO`, the language-agnostic form
  is `languageAgnosticNaturalKey`), `type` (`video` or `audio`), `title`,
  `duration` (seconds), `firstPublished`, `images`, `files[]`.
- `files[]`, one per quality: `progressiveDownloadURL` (on
  `cfp2.jw-cdn.org`), `checksum` (MD5), `filesize`, `label` (`240p` to
  `720p`; **`null` for audio**), `frameHeight`, `mimetype`, `duration` and
  `subtitles.url` (WebVTT, sometimes present with `subtitled: false`).
  `label` is not the real height: `480p` has `frameHeight` 540.
- `images`: `{kind: {size: url}}` with kinds `wss`/`lsr`/`wsr` (wide),
  `sqr`/`sqs` (square) and `pnr` (panorama), sizes `sm`, `md`, `lg`, `xl`. They are
  on `https://cms-imgp.jw-cdn.org/img/…`.
- Files and VTT answer with `access-control-allow-origin: *` and `Range`
  requests, so a webview can stream them and load captions.

Pergament caches each response for six hours under `mediator/` in the cache
folder and uses a stale copy when the network fails. It saves one quality of a
recording per language under `media/` in the library and records it in the
`media` table of `index.sqlite`; the MD5 and size from the catalog are
verified, the captions are saved next to the file.

## Dated texts inside publications

Publications carry their own `DatedText` table: `DocumentId`, `Link`,
`FirstDateOffset`/`LastDateOffset` (`YYYYMMDD` integers),
`BeginParagraphOrdinal`/`EndParagraphOrdinal`, `Caption` and an encrypted
`Content` (same encoding as `Document.Content`). [own]

- Daily text (`es26`): one row per day; `DocumentId` is the month,
  `Content` is the day (date heading, `p.themeScrp`, comment).
- Workbook (`mwb`): one row per week; `DocumentId` is the week's program.
- Study edition (`w`): one row per week; `DocumentId` is the table of
  contents (`Document.Class` 68) and `Content` is its line for the week, with
  a `jwpub://p/{lang}:{MepsDocumentId}/` link to the study article.

The catalog's `PublicationDocument` (`DocumentId` = MEPS document id,
`PublicationId`) says which publication contains a document; it is used to
offer downloads for links into missing publications. [own]

## User data (.jwlibrary backups)

Checked against one backup made by JW Library on iOS (schema version 16,
2026-10). The backup itself is personal and not committed. [own]

- A ZIP with `manifest.json`, `userData.db` and `default_thumbnail.png`.
- `manifest.json`: `creationDate`, `version` (1), `name`, `type` (0) and
  `userDataBackup` {`lastModifiedDate`, `hash`, `databaseName`,
  `deviceName`, `schemaVersion`}. `PRAGMA user_version` of the database
  equals `schemaVersion`.
- The `hash` is not the SHA-256, SHA-1 or MD5 of `userData.db` in the iOS
  backup, so it is not verified on restore. Backups written here use the
  SHA-256 of the database.
- `Location`: the page. Bible chapters have `KeySymbol` (e.g. `nwtsty`),
  `MepsLanguage`, `BookNumber`, `ChapterNumber`; publication documents have
  `KeySymbol`, `IssueTagNumber`, `MepsLanguage` and `DocumentId` = MEPS
  document id. `KeySymbol` is the publication's `UndatedSymbol` when it has
  an issue tag (`w`, `mwb`) and its `Symbol` otherwise (`es26`, `nwtsty`).
  `Type` 0 is a text page (it needs page details); 1 a whole publication,
  including books, magazines and Bibles; 2/3 audio/video.
- `UserMark`: a highlight with `ColorIndex` 1-6, `StyleIndex` (0 in all
  data seen), `UserMarkGuid` (uppercase UUID v4) and `LocationId`.
- `BlockRange`: what a highlight covers. `BlockType` 1 = paragraph
  (`Identifier` = paragraph id, `p{n}` in the HTML), 2 = Bible verse
  (`Identifier` = verse number). `StartToken`/`EndToken` are inclusive token
  indices within the block.
- Tokens: every run of letters, digits and combining marks is one token,
  every other non-space character is one token; verse numbers, paragraph
  numbers and footnote and cross-reference markers do not count. Checked by
  hand against highlights in Psalms 8 and 23, Joshua 1, Ruth 1 and a workbook
  week; e.g. "1. Die richtige Ansicht …" highlighted from token 2 starts at
  "Die". A `-` or `:` between two word characters stays inside the token
  (`Neue-Welt-Übersetzung`, `22:1-3` are one token each), and a soft hyphen
  (U+00AD) is not counted. These came from a backup written by JW Library:
  with them all 38 paragraph highlights of the installed publications start
  and end on sentence boundaries, without them 7 do not. Whether runs of
  punctuation such as `.“` are one token or two is not settled (both readings
  fit the data).
- `Note`: `Guid`, optional `UserMarkId` (the highlight it belongs to),
  `LocationId`, `Title`, `Content` (plain text), `LastModified`, `Created`,
  `BlockType`/`BlockIdentifier` (0 = whole page, else as in `BlockRange`).
- `Tag` (`Type` 1 = user tag, 0 = favorites) and `TagMap` linking a tag to
  a note, a location or a playlist item, with a `Position` per tag.
  Pergament's publication favorites use type-1 locations with no page or
  track details, identified by key symbol, MEPS language and issue tag.
  Imported and newly added favorites share the saved `TagMap.Position`
  order across languages; downloading does not change it. Removing a favorite
  deletes only its type-0 tag mapping, not its location or study data.
  Favorites live exclusively in `userData.db`, including backups and sync;
  old localStorage favorites are ignored.
- `Bookmark` (`Slot`, `Title`, `Snippet`, block), `InputField` (answers
  typed into workbook and study article fields: `TextTag` = the `id` of the
  field's `<textarea>` or `<input>`, e.g. `tt47`, not the `id` of its
  `div.gen-field`; a checkbox's `Value` is `1` or `0`, a text line's the text,
  as in a JW Library backup with checked goals and dates from `lff`),
  playlists and `IndependentMedia` (not used here yet).
- `LastModified` holds one timestamp that the app's triggers update.

### Merging

How Pergament merges a backup into the current data (setting "Merge backups
instead of replacing"). These are Pergament's own rules, not JW Library
behavior. [own]

- Before anything is written, the current database is copied to
  `userData.db.before-restore` (an older copy is replaced). The merge runs in
  one transaction; on an error nothing changes.
- Locations are matched the way the editor finds a page: same `Type`,
  `Track`, `KeySymbol`, `MepsLanguage` and `IssueTagNumber` (NULL equals
  NULL), and same `DocumentId`, `BookNumber` and `ChapterNumber` with NULL
  counted as 0. Failing that, a row with the same value in one of the UNIQUE
  keys (`BookNumber`, `ChapterNumber`, `KeySymbol`, `MepsLanguage`, `Type`)
  or (`KeySymbol`, `IssueTagNumber`, `MepsLanguage`, `DocumentId`, `Track`,
  `Type`) is the same location. Ids are remapped; a missing location is added.
- Matching keys: highlights by `UserMarkGuid`, notes by `Guid`, tags by
  (`Type`, `Name`), bookmarks by (`PublicationLocationId`, `Slot`). Rows with
  a new key are added.
- Input fields are matched by page and `TextTag` against the row the editor
  shows: of the page's locations, a location without a language included
  (JW Library leaves it empty), the one with the lowest `LocationId`. A
  backup row that its own editor would not show, because the backup has the
  same field on an earlier location of the page, is left out.
- A highlight with another `UserMarkGuid` but the same location, color and
  ranges as a local one is the same highlight: nothing is added, and notes
  of the backup's highlight are linked to the local one.
- Recency for a row with the same key and different content: notes compare
  their own `Note.LastModified`; highlights, bookmarks and input fields
  compare the database-wide `LastModified` of the two databases. Timestamps
  are compared as instants with `julianday`, so `Z` and `+00:00` both work.
  A newer backup row replaces the local one; a highlight's ranges and a
  note's tags are replaced together with the row. A note that wins keeps its
  local highlight when the backup's highlight was left out.
- When the backup's highlight, bookmark or input field is not newer (older,
  equal, or with a timestamp that cannot be parsed, e.g. `+0000` without a
  colon), the local row is kept and the backup's is reported as a conflict,
  since the database-wide clock may not say which row was edited last. A
  note that is older by its own timestamp is kept without a report; equal
  or unparseable note timestamps with different text are a conflict.
- A new highlight that overlaps a local one with another `UserMarkGuid`
  follows the newer database: if the incoming one is newer, the local ranges
  are cut back around it; otherwise it is skipped and reported as a conflict.
- Location-level `TagMap` rows (favorites) are unioned. A note's tags are
  taken over with the note.
- After a merge that added or replaced something, the database-wide
  `LastModified` becomes the later of the two databases' values; it is not set
  to the time of the merge, so merging an old backup does not make the data
  look newer than they are. A merge that changes nothing leaves it alone.
- Deletions are not propagated by a manual merge: a row missing on one side
  stays, so an item deleted on one side comes back when the other side's
  backup is merged. Folder sync does pass them on (see "Sync").
- Playlists and `IndependentMedia` are not merged.

#### Sync

Folder sync merges another computer's file with the same steps, but
three-way: against the *base*, a snapshot of that computer's file as it was
at the last merge, kept in the app's data folder. [own]

- A snapshot holds every highlight (with its ranges), note, user tag,
  bookmark, answer field and note or location `TagMap` row under a key that
  does not depend on database ids, with a hash of its content. The keys are
  the merge's: `UserMarkGuid`, note `Guid`, tag (`Type`, `Name`), bookmark
  publication location and `Slot`, answer page (without its language) and
  `TextTag`, and for a `TagMap` row its tag and its note `Guid` or location.
  Locations are compared normalized as above, so the same row has the same
  key and hash in every database. The database-wide `LastModified` and a
  note's `LastModified` are not part of the hash.
- Each key is compared in the base (B), the incoming file (I) and the local
  data (L):

  | B | I | L | Result |
  |---|---|---|---|
  | – | present | – | added |
  | – | – | present | kept |
  | – | present | present | nothing to do when equal, otherwise see below, conflict |
  | present | missing | unchanged | deleted here |
  | present | missing | changed | kept, conflict |
  | present | unchanged | missing | stays deleted |
  | present | changed | missing | added back, conflict |
  | present | unchanged | present | local row kept |
  | present | changed | unchanged | incoming row taken |
  | present | changed | changed the same way | local row kept |
  | present | changed | changed differently | see below, conflict |

  An edit wins over a deletion on either side. A row changed only here is
  never replaced by an older copy from the other computer, whatever the
  database-wide `LastModified` says.
- A row changed differently on both sides, or new on both sides with
  different content (e.g. the same bookmark slot), is settled the same way
  on every computer: a note by its own `LastModified`, the later one kept; a row
  without a clock of its own (highlight, bookmark, answer, tag), or a note
  changed at the same time, by its snapshot hash, the version with the
  greater hash (compared as text) kept. The database-wide `LastModified`
  is not used, as each computer would see itself as newer and keep its own
  version for good. The row is reported as a conflict.
- A note's tags and a tag's notes are `TagMap` rows of their own. A note or
  tag deleted on one side and given a new `TagMap` row on the other counts
  as changed: it is kept (or added back) with all its `TagMap` rows, and
  reported. A note or tag added back brings its `TagMap` rows with it.
- The order of a tag's notes is not merged. A `TagMap` row's hash is its
  rank in the tag, but the merge only asks whether the row is there, so
  moving a note within a tag is neither a change nor a conflict, and two
  computers can keep different orders. A row added to a tag goes to its end.
- Deletions run first, in the merge's transaction, the way the editor
  deletes: a highlight's notes stay without it and its ranges go with it; a
  note's `TagMap` rows go with the note; a tag is deleted only when no
  `TagMap` row refers to it any more, otherwise it is kept and reported; an
  answer is deleted from every location of its page, as in the editor. A
  deleted row is missing from the incoming file by definition, so the merge
  that follows never adds it back or updates it.
- Should two rows of one database share a key (e.g. two `Location` rows
  that normalize to the same page), the snapshot holds the first by id.
  Only that row is ever deleted (an answer with the field on the other
  locations of its page), and only that row is merged by these
  rules; the other is merged by the manual merge's rules. A row left over
  this way shows up in the next snapshot as new here and is passed on, so it
  is never lost.
- A new incoming highlight that overlaps a local one still follows the
  newer database, as in a manual merge.
- Without a base (the first merge of a computer's file), the merge is a
  manual merge: nothing is deleted.
- The deletions are checked for each kind of row apart: highlights and
  notes, answers, bookmarks, and tags with their `TagMap` rows. When in any
  kind they are more than half of the local rows of that kind and more than
  5 rows, the merge stops before writing anything, including the safety
  copy, and asks, giving the deletions and local rows summed over all
  kinds. "Apply deletions" runs it as
  computed, after a rolling backup of the local data (written even when
  rolling backups are off); if that backup fails, the deletions wait and
  the sync status shows the error. "Keep my data" runs it without deletions, and
  the merged file still becomes the new base. That protects only this
  computer: the rows the other computer deleted stay deleted there, as this
  computer's own file now holds them unchanged against its base on the
  other side. To recover them on the other computer, restore a backup there.
- A computer whose synced rows are all gone (no highlight, note, user tag,
  bookmark or answer) while its bases still hold rows lost or reset its
  database. It is treated as new: before merging, it drops its bases and
  its sync state, so every merge only adds. Empty bases (two empty
  computers) do not count, so nothing is merged again and again.
- A computer never writes its own file while it has no synced rows: another
  computer may hold a base for that file (also from before a "Stop
  syncing", which keeps the device id), and an empty file would delete its
  rows. It writes once it has rows, e.g. after a merge refilled it, so a
  round in which the other files are missing or unreadable writes nothing.
  Deleting every row on purpose is treated the same way: the file is not
  written and the rows come back from the other computers.
- "Stop syncing" drops the bases. Syncing again merges without a base,
  which only adds: rows deleted elsewhere in the meantime are not deleted
  here (the other computers keep them deleted, as they are unchanged
  against their bases).
- A restore writes a rolling backup of the data it replaces (even when
  rolling backups are off): the safety copy `userData.db.before-restore`
  is replaced by the next merge.
- Rolling backups (`Pergament-YYYY-MM-DD-HHMMSS.jwlibrary`) are never read
  as another computer's file, and the library folder and its `backups`
  folder cannot be the sync folder. The host part of the own file name is
  stored with the device id, so a renamed computer keeps its file.
- The snapshot of the merged file becomes the new base only after the merge
  has committed. Merging the same file again, or a file equal to the base,
  changes nothing.

## Unknown / not relied upon

- What the `C` constant protects beyond obfuscation. We only reproduce the
  documented algorithm.
- `BibleVerse.AdjustmentInfo` meaning.
- Exact semantics of `data-xtid` (likely the per-document extract number;
  not confirmed), `data-bid` (looks like `{block}-{element}`) and
  `data-vlid`.
- `BibleCitation.SortPosition`.
- The suffix in publication links like `jwpub://p/X:1001070144/1-1`.
- `VerseMultimediaMap` (media attached to verses) is not shown yet; only recordings linked from the text, the book intro videos and those in the Video and Audio tabs play.
- Full list of `Document.Class`/`Type` values across other publication types.
- `contentFormat` values other than `z-a`.
- Whether older `schemaVersion`s (e.g. 8 in [jwapi]) differ in a way that matters.
- Whether `GETPUBMEDIALINKS` ever returns more than one JWPUB file per
  language; Pergament takes the first.
- Rate limits on the jw.org side (none hit at one request per second).
