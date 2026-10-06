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
  `Type` 0 is text; 1 a whole Bible; 2/3 audio/video.
- `UserMark`: a highlight with `ColorIndex` 1-6, `StyleIndex` (0 in all
  data seen), `UserMarkGuid` (uppercase UUID v4) and `LocationId`.
- `BlockRange`: what a highlight covers. `BlockType` 1 = paragraph
  (`Identifier` = paragraph id, `p{n}` in the HTML), 2 = Bible verse
  (`Identifier` = verse number). `StartToken`/`EndToken` are inclusive token
  indices within the block.
- Tokens: every run of letters, digits and combining marks is one token,
  every other non-space character is one token; verse numbers and footnote
  and cross-reference markers do not count. Checked by hand against
  highlights in Psalms 8 and 23, Joshua 1, Ruth 1 and a workbook week; e.g.
  "1. Die richtige Ansicht …" highlighted from token 2 starts at "Die".
  Whether runs of punctuation such as `.“` are one token or two is not
  settled (both readings fit the data).
- `Note`: `Guid`, optional `UserMarkId` (the highlight it belongs to),
  `LocationId`, `Title`, `Content` (plain text), `LastModified`, `Created`,
  `BlockType`/`BlockIdentifier` (0 = whole page, else as in `BlockRange`).
- `Tag` (`Type` 1 = user tag, 0 = favorites) and `TagMap` linking a tag to
  a note, a location or a playlist item, with a `Position` per tag.
- `Bookmark` (`Slot`, `Title`, `Snippet`, block), `InputField` (answers
  typed into workbook fields: `TextTag` = the field id, e.g. `tt21`),
  playlists and `IndependentMedia` (not used here yet).
- `LastModified` holds one timestamp that the app's triggers update.

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
- Full list of `Document.Class`/`Type` values across other publication types.
- `contentFormat` values other than `z-a`.
- Whether older `schemaVersion`s (e.g. 8 in [jwapi]) differ in a way that matters.
- Whether `GETPUBMEDIALINKS` ever returns more than one JWPUB file per
  language; Pergament takes the first.
- Rate limits on the jw.org side (none hit at one request per second).
