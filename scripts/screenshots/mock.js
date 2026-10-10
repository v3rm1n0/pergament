// Stand-in for the Tauri backend, used to take the README screenshots in a
// plain browser (see scripts/screenshots/run.mjs and CONTRIBUTING.md).
//
// Everything in here is invented: publication titles, Bible text, notes,
// covers. No real publication text or artwork may be added to this file.
//
// Plain browser script without imports; inject it with
// `context.addInitScript({ path })` so it runs before the app loads.
(() => {
  "use strict";

  const missing = [];
  window.__mockMissing = missing;
  window.isTauri = true;

  // ---------- Covers: flat SVGs, palette and motif differ from tile to tile ----------

  const PALETTES = [
    ["#1f4e5f", "#e8c547", "#f4f1de", "#d1495b"],
    ["#3d2c4d", "#f2a65a", "#eec170", "#58a4b0"],
    ["#2b5d34", "#f0e6c8", "#a7c957", "#bc4749"],
    ["#7a2e3b", "#f6d9a8", "#e9b44c", "#2e5266"],
    ["#22333b", "#c6ac8f", "#eae0d5", "#5e503f"],
    ["#0b3954", "#bfd7ea", "#ff6663", "#e0ff4f"],
    ["#5f0f40", "#fb8b24", "#e36414", "#9a031e"],
    ["#264653", "#2a9d8f", "#e9c46a", "#f4a261"],
    ["#3c1642", "#86bbd8", "#e8d6cb", "#33658a"],
    ["#4a4e69", "#c9ada7", "#f2e9e4", "#9a8c98"],
    ["#14213d", "#fca311", "#e5e5e5", "#8d99ae"],
    ["#606c38", "#fefae0", "#dda15e", "#bc6c25"],
  ];

  const MOTIFS = [
    (c) => `<circle cx="100" cy="86" r="42" fill="${c[1]}"/><rect y="128" width="200" height="72" fill="${c[2]}"/><rect y="150" width="200" height="14" fill="${c[3]}"/>`,
    (c) =>
      [0, 1, 2, 3].map((i) => `<path d="M0 ${70 + i * 32} q25 -26 50 0 t50 0 t50 0 t50 0 V200 H0Z" fill="${c[1 + (i % 3)]}" opacity="${0.95 - i * 0.12}"/>`).join(""),
    (c) => `<circle cx="50" cy="150" r="80" fill="${c[1]}"/><circle cx="150" cy="170" r="70" fill="${c[2]}"/><circle cx="110" cy="60" r="26" fill="${c[3]}"/>`,
    (c) => [0, 1, 2, 3, 4].map((i) => `<path d="M${-60 + i * 56} 200 L${40 + i * 56} 0 h22 L${-38 + i * 56} 200Z" fill="${c[1 + (i % 3)]}"/>`).join(""),
    (c) => [0, 1, 2].flatMap((r) => [0, 1, 2].map((q) => `<rect x="${20 + q * 56}" y="${20 + r * 56}" width="48" height="48" fill="${c[1 + ((r + q) % 3)]}"/>`)).join(""),
    (c) => [80, 62, 44, 26].map((r, i) => `<path d="M${100 - r} 170 a${r} ${r} 0 0 1 ${r * 2} 0" fill="none" stroke="${c[1 + (i % 3)]}" stroke-width="12"/>`).join(""),
    (c) => `<path d="M0 170 L60 70 L110 140 L150 90 L200 170Z" fill="${c[1]}"/><path d="M0 200 L70 120 L130 180 L170 140 L200 200Z" fill="${c[2]}"/><circle cx="150" cy="40" r="16" fill="${c[3]}"/>`,
    (c) => [0, 1, 2, 3, 4, 5].flatMap((r) => [0, 1, 2, 3, 4, 5].map((q) => `<circle cx="${22 + q * 31}" cy="${22 + r * 31}" r="${5 + ((r * q) % 4) * 2}" fill="${c[1 + ((r + q) % 3)]}"/>`)).join(""),
    (c) => `<path d="M50 190 V90 a50 50 0 0 1 100 0 V190Z" fill="${c[1]}"/><path d="M76 190 V96 a24 24 0 0 1 48 0 V190Z" fill="${c[2]}"/><rect x="0" y="176" width="200" height="24" fill="${c[3]}"/>`,
    (c) => [0, 1, 2].map((i) => `<ellipse cx="${60 + i * 40}" cy="${100 + (i % 2) * 18}" rx="22" ry="62" transform="rotate(${-30 + i * 30} ${60 + i * 40} 100)" fill="${c[1 + i]}"/>`).join(""),
  ];

  let coverCount = 5; // 0-4 belong to the five publications
  /** A cover as a data URI; every call picks a different palette and motif than the previous one. */
  function cover(pick) {
    const i = pick ?? coverCount++;
    const c = PALETTES[(i * 5 + 1) % PALETTES.length];
    const motif = MOTIFS[(i * 3) % MOTIFS.length];
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200"><rect width="200" height="200" fill="${c[0]}"/>${motif(c)}</svg>`;
    return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
  }

  const banner = (a, b, c) =>
    `data:image/svg+xml;utf8,${encodeURIComponent(
      `<svg xmlns="http://www.w3.org/2000/svg" width="600" height="150" viewBox="0 0 600 150"><rect width="600" height="150" fill="${a}"/><circle cx="470" cy="60" r="34" fill="${c}"/><path d="M0 150 L120 70 L230 130 L330 60 L450 140 L600 80 V150Z" fill="${b}"/><path d="M0 150 L90 112 L190 150Z" fill="${c}" opacity=".5"/></svg>`,
    )}`;

  // ---------- Bibles: about 36 invented books ----------

  const BOOK_NAMES = [
    "Tides", "Lanterns", "Ashes", "Orchards", "Ferrymen", "Beacons", "Thresholds", "Salt", "Embers", "Meadows", "Cartographers", "Kites",
    "Rivers", "Quarries", "Bellows", "Harvests", "Weavers", "Cisterns", "Watchmen", "Moorings", "Lamplighters", "Granaries", "Compasses",
    "Footbridges", "Windmills", "Anchors", "Mosaics", "Foundries", "Echoes", "Hearths", "Glassblowers", "Sailmakers", "Pilgrims",
    "Reservoirs", "Evenings", "Dawn",
  ];
  const SPLIT = 20; // books 1-20 in the first part
  const chaptersOf = (b) => (b === 2 ? 12 : b === 1 ? 8 : 4 + ((b * 7) % 9));
  const bookTitle = (b) => BOOK_NAMES[b - 1];

  const bookNode = (b) => ({ title: bookTitle(b), document_id: null, bible_book: b, children: [] });
  const bibleToc = () => [
    {
      title: "Books",
      document_id: null,
      bible_book: null,
      children: [
        { title: "First Scrolls", document_id: null, bible_book: null, children: BOOK_NAMES.slice(0, SPLIT).map((_, i) => bookNode(i + 1)) },
        { title: "Later Letters", document_id: null, bible_book: null, children: BOOK_NAMES.slice(SPLIT).map((_, i) => bookNode(SPLIT + i + 1)) },
      ],
    },
    {
      title: "Appendix",
      document_id: null,
      bible_book: null,
      children: [
        { title: "How the books are arranged", document_id: 9001, bible_book: null, children: [] },
        { title: "Weights, measures and lamps", document_id: 9002, bible_book: null, children: [] },
      ],
    },
  ];
  const bibleBooks = () =>
    BOOK_NAMES.map((name, i) => ({ number: i + 1, title: name, chapter_title: name, book_document_id: null, chapters: chaptersOf(i + 1) }));

  // The featured chapter, Lanterns 10, in two renditions. Footnote and
  // cross-reference markers sit after the verse text.
  const L10 = {
    lbn: [
      "A lamp that is lit in the morning has little to prove, for the day itself bears witness to it.",
      "Yet those who keep their lamps hidden in cupboards say that darkness is wiser than flame.",
      "Do not despise the small oil that is poured out in secret; it fills the vessel before the guests arrive.",
      "Carry the lamp ahead of you, and the road will answer.",
      "The ferryman does not ask the river whether it wishes to be crossed; he reads the water and sets his pole.",
      "So read the season before you plant, and the harvest will not mock your patience.",
      "A neighbour’s gate stands open for the one who knocks gently, and closed against the one who counts the hours.",
      "Share your bread while it is warm, for warmth is the part that cannot be stored.",
      "Those who mend the nets of others will find their own nets whole at the turning of the tide.",
      "Let no one say, “The work is too large,” for the wall was raised one stone at a time.",
      "When the wind turns, do not curse the sail; trim it, and let the ship learn a new road.",
      "A quiet word at dusk outweighs a loud speech at noon.",
      "Keep the lamp trimmed, the door unlatched, and the table set, and you will never be a stranger in your own house.",
      "For the light you give away returns before you have finished giving.",
    ],
    prb: [
      "A lamp lit at dawn needs no defence; the daylight speaks for it.",
      "Some people store their lamps in cupboards and then praise the dark for its good sense.",
      "Do not look down on the little oil poured in private; it has filled the lamp long before company comes.",
      "Take the lamp in front of you as you walk, and the road will reply.",
      "A ferryman never asks the river for permission; he studies the current and plants his pole.",
      "Study the season first and then sow, and the harvest will not laugh at your patience.",
      "A neighbour’s gate swings wide for a gentle knock and stays shut for anyone watching the clock.",
      "Give out your bread while it is warm; warmth is the one thing you cannot put in a cellar.",
      "Whoever repairs another person’s nets will find their own mended when the tide turns.",
      "Nobody should say, “The task is too big,” because the wall went up a single stone at a time.",
      "If the wind shifts, do not scold the sail. Adjust it and let the ship find another way.",
      "One soft word at dusk weighs more than a shout at midday.",
      "Trim the lamp, leave the door on the latch and lay the table, and you will never feel a guest at home.",
      "The light you hand to others comes back to you before your hands are empty.",
    ],
  };

  const POOL = [
    "The path is longer than the map admits, and shorter than the weary believe.",
    "A patient hand weighs the grain twice and the promise once.",
    "Those who listen at the door will be welcome at the table.",
    "Every bridge was first an idea about the other side.",
    "Water finds the lowest place, and so does a kind word.",
    "He who guards the fire at night is owed a share of the morning.",
    "Do not measure the field by the fence, but by the hands that tend it.",
    "A borrowed coat keeps the cold out and the debt in.",
    "The market is loud, but the well is deep and quiet.",
    "Sow generously; the sparrows also have families.",
    "A crooked furrow still feeds the household.",
    "Even the strongest rope began as many small threads.",
    "Walk beside the one who limps, and the road will seem shorter to both.",
    "The harbour light does not argue with the fog; it simply keeps shining.",
    "A guest remembers the warmth of the room longer than the length of the feast.",
    "Call the question by its name, and half the fear leaves the room.",
    "Bread shared at dusk tastes better than bread kept until morning.",
    "Where there is a lamp, there is also someone who lit it.",
    "The old well has not forgotten the village, though the village has forgotten the well.",
    "Trim your sails to the wind you have, not the wind you were promised.",
    "A wise builder learns from the stones that fell the first time.",
    "Let your yes be plain and your no be kind.",
    "Rest is also part of the harvest.",
    "Count the blessings of the day before you count its losses.",
  ];

  function chapterVerses(dir, book, chapter) {
    if (book === 2 && chapter === 10) return L10[dir === "lbn_E" ? "lbn" : "prb"];
    const shift = dir === "lbn_E" ? 0 : 7;
    const count = 8 + ((book * 3 + chapter) % 7);
    return Array.from({ length: count }, (_, i) => POOL[(book * 5 + chapter * 3 + i * 2 + shift) % POOL.length]);
  }

  // Footnote and cross-reference markers of the featured chapter (Lantern Bible only).
  const MARKERS = { 3: [{ xr: 1, letter: "a" }], 4: [{ fn: 1 }], 5: [{ fn: 2 }], 10: [{ xr: 2, letter: "b" }], 12: [{ xr: 3, letter: "c" }] };

  const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

  function chapterHtml(dir, book, chapter) {
    const verses = chapterVerses(dir, book, chapter);
    const featured = dir === "lbn_E" && book === 2 && chapter === 10;
    const parts = verses.map((text, i) => {
      const v = i + 1;
      const label = v === 1 ? `<span class="cl">${chapter}</span>` : `<span class="vl">${v}</span>`;
      const marks = featured
        ? (MARKERS[v] ?? [])
            .map((m) => (m.fn ? `<a class="fn" href="#footnote${m.fn}" id="footnotesource${m.fn}">*</a>` : `<a class="xr" href="#xref${m.xr}" id="xrefsource${m.xr}">${m.letter}</a>`))
            .join("")
        : "";
      return `<span id="v${book}-${chapter}-${v}-1" class="v">${label}${esc(text)}${marks}</span>`;
    });
    // Two paragraphs, as in a printed Bible.
    const cut = Math.ceil(parts.length / 2);
    const title = `${bookTitle(book)} ${chapter}`;
    return `<article class="bible-chapter"><h2 class="chapter-title">${title}</h2><p>${parts.slice(0, cut).join(" ")}</p><p>${parts.slice(cut).join(" ")}</p></article>`;
  }

  const L10_STUDY = {
    outlineTitle: "Outline of Lanterns",
    outline: [
      { level: 2, text: "Light that does not hide", begin_chapter: 10, begin_verse: 1, end_chapter: 10, end_verse: 4 },
      { level: 3, text: "The lamp in the cupboard", begin_chapter: 10, begin_verse: 2, end_chapter: 10, end_verse: 3 },
      { level: 2, text: "Reading the river and the season", begin_chapter: 10, begin_verse: 5, end_chapter: 10, end_verse: 7 },
      { level: 2, text: "Work done one stone at a time", begin_chapter: 10, begin_verse: 8, end_chapter: 10, end_verse: 11 },
      { level: 2, text: "A home that stays open", begin_chapter: 10, begin_verse: 12, end_chapter: 10, end_verse: 14 },
    ],
    verses: [
      {
        chapter: 10,
        verse: 3,
        footnotes: [],
        xrefs: [{ marker: "a", block: 1, refs: [{ href: "pergament://bible/1:4:2-1:4:3", label: "Tides 4:2, 3" }, { href: "pergament://bible/9:7:1-9:7:1", label: "Embers 7:1" }] }],
        notes: [],
      },
      {
        chapter: 10,
        verse: 4,
        footnotes: [{ marker: "*", index: 1, html: "<p>Or “go before you with the lamp.”</p>" }],
        xrefs: [],
        notes: [
          `<div class="study-note"><p><strong>Carry the lamp ahead of you:</strong> In the lantern-bearing trade of the river towns, the bearer walked a few steps in front so that the group could see the ground before they stepped on it.</p></div>`,
        ],
      },
      {
        chapter: 10,
        verse: 5,
        footnotes: [{ marker: "*", index: 2, html: "<p>Literally “reads the face of the water.”</p>" }],
        xrefs: [],
        notes: [],
      },
      {
        chapter: 10,
        verse: 10,
        footnotes: [],
        xrefs: [{ marker: "b", block: 2, refs: [{ href: "pergament://bible/9:7:11-9:7:11", label: "Embers 7:11" }] }],
        notes: [
          `<div class="study-note"><p><strong>The wall was raised one stone at a time:</strong> City walls in the hill country were built by neighbouring households, each responsible for the stretch in front of its own door.</p></div>`,
        ],
      },
      {
        chapter: 10,
        verse: 12,
        footnotes: [],
        xrefs: [{ marker: "c", block: 3, refs: [{ href: "pergament://bible/29:5:1-29:5:1", label: "Echoes 5:1" }, { href: "pergament://bible/30:1:2-30:1:3", label: "Hearths 1:2, 3" }] }],
        notes: [],
      },
    ],
  };

  const RESEARCH = {
    4: [
      {
        publication: "Harbor Almanac 2024",
        subject: "Lamps and Roads",
        location: "Almanac 2024, pp. 12-13",
        href: "pergament://bible/2:10:4-2:10:4",
        html: "<p>Night travellers in the river towns hired a lamp-bearer, who walked a few paces ahead and called out puddles, steps and loose stones. The fee was a loaf and a coin.</p>",
      },
      {
        publication: "Notes on Everyday Crafts",
        subject: "The oil merchant",
        location: "Everyday Crafts, p. 41",
        href: "pergament://bible/2:10:3-2:10:3",
        html: "<p>Lamp oil was pressed in autumn and sold by the cupful. A household that bought early had light all winter, and the oil merchant kept a ledger of favours as well as of coins.</p>",
      },
    ],
  };

  // ---------- Publications ----------

  const LANG = "E";
  const PUBS = [
    { dir: "lbn_E", title: "The Lantern Bible", shortTitle: "Lantern Bible", symbol: "lbn", year: 2026, issueTag: "0", publicationType: "Bible", isBible: true, cover: cover(0) },
    { dir: "prb_E", title: "The Plain Rendering of the Scrolls", shortTitle: "Plain Rendering", symbol: "prb", year: 2024, issueTag: "0", publicationType: "Bible", isBible: true, cover: cover(1) },
    { dir: "mlc26_E", title: "Morning Light: A Daily Companion", shortTitle: "Morning Light 2026", symbol: "mlc", year: 2026, issueTag: "0", publicationType: "Booklet", isBible: false, cover: cover(2) },
    { dir: "mwb_E_202610", title: "Gathering Workbook, October-November 2026", shortTitle: "Gathering Workbook", symbol: "gwb", year: 2026, issueTag: "202610", publicationType: "Meeting Workbook", isBible: false, cover: cover(3) },
    { dir: "lsq_E_202610", title: "Longshore Quarterly, October 2026", shortTitle: "Longshore Quarterly", symbol: "lsq", year: 2026, issueTag: "202610", publicationType: "Article Series", isBible: false, cover: cover(4) },
  ].map((p) => ({ ...p, mepsLanguage: 0, langCode: LANG }));
  const pub = (dir) => PUBS.find((p) => p.dir === dir);

  const WEEKS = [
    { id: 101, label: "October 5-11" },
    { id: 102, label: "October 12-18" },
    { id: 103, label: "October 19-25" },
    { id: 104, label: "October 26 - November 1" },
  ];

  const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

  function publicationDetail(dir) {
    const card = pub(dir);
    if (!card) throw `unknown publication ${dir}`;
    let toc = [];
    if (card.isBible) toc = bibleToc();
    else if (dir === "mwb_E_202610") {
      toc = [{ title: "Gathering Workbook", document_id: null, bible_book: null, children: WEEKS.map((w) => ({ title: w.label, document_id: w.id, bible_book: null, children: [] })) }];
    } else if (dir === "lsq_E_202610") {
      toc = [{ title: "Longshore Quarterly", document_id: null, bible_book: null, children: [{ title: "Why Small Kindnesses Outlast Grand Gestures", document_id: 201, bible_book: null, children: [] }] }];
    } else {
      toc = [{ title: "Morning Light 2026", document_id: null, bible_book: null, children: MONTHS.map((m, i) => ({ title: m, document_id: 300 + i, bible_book: null, children: [] })) }];
    }
    return { card, toc, books: card.isBible ? bibleBooks() : [] };
  }

  // ---------- Daily text ----------

  const DAILY = [
    {
      scripture: "Carry the lamp ahead of you, and the road will answer.",
      ref: { href: "pergament://bible/2:10:4-2:10:5", label: "Lanterns 10:4, 5" },
      paragraphs: [
        "Anyone who has walked home after dark knows the difference between a lamp held behind and a lamp held ahead. The first lights the traveller’s own feet; the second lights the way for everyone who follows.",
        "Think of the small courtesies that show the path to others: a greeting on the stairs, a note left on a door, the patience to explain the route twice. None of them costs much oil, and all of them add light (<a class=\"b\" href=\"pergament://bible/1:4:2-1:4:3\">Tides 4:2, 3</a>).",
        "<em>Today, whose road could you light a few steps ahead?</em>",
      ],
    },
    {
      scripture: "Water finds the lowest place, and so does a kind word.",
      ref: { href: "pergament://bible/1:3:2-1:3:2", label: "Tides 3:2" },
      paragraphs: [
        "A kind word does not need a stage. It runs downhill to wherever someone is feeling low, and settles there.",
        "A neighbour who has had a long week may not remember what you said, but will remember that someone said anything at all.",
      ],
    },
    {
      scripture: "Rest is also part of the harvest.",
      ref: { href: "pergament://bible/16:5:3-16:5:3", label: "Harvests 5:3" },
      paragraphs: [
        "Farmers know that a field left fallow is not a field wasted. The soil is mending while the calendar looks empty.",
        "Make room for an unhurried evening this week, and treat it as part of the work rather than a break from it.",
      ],
    },
  ];

  const pad = (n) => String(n).padStart(2, "0");
  const dateNum = (s) => Number(s.replaceAll("-", ""));
  const isoOf = (n) => `${Math.floor(n / 10000)}-${pad(Math.floor(n / 100) % 100)}-${pad(n % 100)}`;
  const longDate = (n) => new Date(`${isoOf(n)}T12:00:00`).toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" });

  function dailyHtml(n) {
    const day = Math.floor(new Date(`${isoOf(n)}T12:00:00`).getTime() / 86_400_000);
    const entry = n === 20261007 ? DAILY[0] : DAILY[((day % DAILY.length) + DAILY.length) % DAILY.length];
    const [first, ...rest] = entry.paragraphs;
    return (
      `<h2>${longDate(n)}</h2>` +
      `<p class="themeScrp" id="p1">“${esc(entry.scripture)}”—<a class="b" href="${entry.ref.href}">${entry.ref.label}</a></p>` +
      `<p id="p2">${first}</p>` +
      rest.map((p, i) => `<p id="p${i + 3}">${p}</p>`).join("")
    );
  }

  // ---------- Workbook and study article ----------

  const WORKBOOK_HTML = (label) =>
    `<header><h1>${label}</h1></header>` +
    `<h2 id="p1">Reading: Tides 3-4</h2>` +
    `<h3 id="p2">Opening song and welcome (1 min.)</h3>` +
    `<h2 id="p3">Gems of the Week</h2>` +
    `<h3 id="p4">1. Learning to Read the Weather of a Conversation</h3>` +
    `<p id="p5">(10 min.)</p>` +
    `<p id="p6">Patient listeners notice the mood of a room before they say a word (<a class="b" href="pergament://bible/1:3:2-1:3:4">Tides 3:2-4</a>).</p>` +
    `<p id="p7">A question asked at the right moment can open a door that a long speech would close, although the speaker may feel clever either way (<a class="b" href="pergament://bible/2:10:5-2:10:6">Lanterns 10:5, 6</a>).</p>` +
    `<p id="p8">Short sentences travel well and a pause is often the most generous thing you can offer to a tired neighbour.</p>` +
    `<p id="p9"><strong>FOR REFLECTION:</strong> Which of my habits help other people feel heard?—<a class="b" href="pergament://bible/4:2:5-4:2:5">Orchards 2:5</a>.</p>` +
    `<h3 id="p10">2. Digging for Gems</h3>` +
    `<p id="p11">What does the image of a mended net teach us about helping others (<a class="b" href="pergament://bible/2:10:9-2:10:9">Lanterns 10:9</a>)?</p>` +
    `<h2 id="p12">Practice Session</h2>` +
    `<h3 id="p13">3. First Conversation (3 min.)</h3>` +
    `<p id="p14">Greet a neighbour at the market and offer to carry one bag. Rehearse a friendly opening and a graceful exit.</p>` +
    `<img src="${banner("#2b5d34", "#a7c957", "#f0e6c8")}" alt="" style="width:100%;margin:0 0 .9em"/>` +
    `<h2 id="p15">Community Matters</h2>` +
    `<h3 id="p16">4. Local Needs (15 min.)</h3>` +
    `<p id="p17">Discussion led by the chairman. Which small job could we finish together this month, and who has the right tools in the shed?</p>` +
    `<section class="footnotes"><h2>Footnotes</h2><div class="footnote"><a class="fn-back" href="#footnotesource1">*</a><p id="footnote1">Chairs are asked to keep the discussion to the time shown.</p></div></section>`;

  const STUDY_HTML =
    `<header><h1>Why Small Kindnesses Outlast Grand Gestures</h1><p class="subtitle">Longshore Quarterly, October 2026</p></header>` +
    `<p id="p1">A grand gesture is loud on the day and quiet afterwards. A small kindness is quiet on the day and loud for years.</p>` +
    `<p id="p2">Consider the baker who keeps a loaf back for the family on the corner, or the neighbour who shovels two paths instead of one. Neither will be mentioned in a speech.</p>` +
    `<p id="p3">Yet both are remembered when the speeches are forgotten (<a class="b" href="pergament://bible/2:10:8-2:10:8">Lanterns 10:8</a>).</p>`;

  // ---------- Catalog entries ----------

  const entryOf = (symbol, category, categoryId, title, { issue = 0, size = 3_400_000, shortTitle = null, issueTitle = null, on = "2026-09-20T08:00:00Z", pick, local = null } = {}) => {
    const img = local ? pub(local).cover : cover(pick);
    return {
      item: {
        key_symbol: symbol, symbol, meps_language: 0, issue_tag: issue, year: 2026, title, issue_title: issueTitle, size, sha1: "",
        publication_type: categoryId, short_title: shortTitle, cataloged_on: on, image: img, attributes: [],
      },
      category,
      imageUrl: img,
      local,
    };
  };

  const BROCHURES = "Brochures and Booklets";
  const TOOLBOX = [
    entryOf("nlk", BROCHURES, 4, "Notes From the Lighthouse Keeper", { shortTitle: "Lighthouse Keeper" }),
    entryOf("pgl", BROCHURES, 4, "A Pocket Guide to Patient Listening", { shortTitle: "Patient Listening" }),
    entryOf("gss", BROCHURES, 4, "Gardening in Small Spaces"),
    entryOf("lys", BROCHURES, 4, "Letters to a Younger Self"),
    entryOf("toy", "Books", 2, "The Orchard Year"),
    entryOf("mwh", "Books", 2, "Maps for Walking Home"),
    entryOf("rtm", "Books", 2, "What the River Taught the Mill", { shortTitle: "River and Mill" }),
    entryOf("ial", "Tracts and Invitations", 10, "Is Anyone Listening?", { size: 600_000 }),
    entryOf("fqh", "Tracts and Invitations", 10, "Five Quiet Habits", { size: 600_000 }),
    entryOf("mrt", "Tracts and Invitations", 10, "Making Room at the Table", { size: 600_000 }),
    entryOf("skn", "Tracts and Invitations", 10, "A Short Note on Kindness", { size: 600_000 }),
  ];

  const LOCAL_ENTRIES = {
    mlc26_E: entryOf("mlc", BROCHURES, 4, "Morning Light: A Daily Companion", { shortTitle: "Morning Light 2026", local: "mlc26_E", on: "2026-09-29T08:00:00Z" }),
    mwb_E_202610: entryOf("gwb", "Meeting Workbooks", 30, "Gathering Workbook, October-November 2026", { issue: 202610, issueTitle: "Gathering Workbook, October-November 2026", local: "mwb_E_202610", on: "2026-10-02T08:00:00Z" }),
    lsq_E_202610: entryOf("lsq", "Article Series", 22, "Longshore Quarterly", { issue: 202610, issueTitle: "Longshore Quarterly, October 2026", local: "lsq_E_202610", on: "2026-10-05T08:00:00Z", size: 5_100_000 }),
  };

  const WHATS_NEW = [
    LOCAL_ENTRIES.lsq_E_202610,
    LOCAL_ENTRIES.mwb_E_202610,
    entryOf("ste", "Books", 2, "Study Notes for the Long Evenings", { on: "2026-10-03T08:00:00Z", size: 9_800_000 }),
    entryOf("hpg", BROCHURES, 4, "Hearth and Home: A Practical Guide", { on: "2026-09-30T08:00:00Z" }),
    entryOf("cgp", "Programs", 31, "Circuit Gathering Program 2026", { on: "2026-09-24T08:00:00Z", size: 700_000 }),
    entryOf("tbn", "Tracts and Invitations", 10, "The Bridge Between Neighbours", { on: "2026-09-18T08:00:00Z", size: 600_000 }),
  ];

  const MORE_STUDY = [
    entryOf("cvn", "Programs", 31, "Convention Notes 2026", { on: "2026-08-02T08:00:00Z", size: 1_200_000 }),
    entryOf("wbo", "Meeting Workbooks", 30, "Gathering Workbook, November-December 2026", { issue: 202611, on: "2026-10-04T08:00:00Z" }),
  ];

  const ALL_CATALOG = [...TOOLBOX, ...Object.values(LOCAL_ENTRIES), ...WHATS_NEW, ...MORE_STUDY];

  const CATEGORIES = [
    { id: 2, name: "Books", count: 31 },
    { id: 4, name: "Brochures and Booklets", count: 54 },
    { id: 10, name: "Tracts and Invitations", count: 22 },
    { id: 22, name: "Article Series", count: 9 },
    { id: 30, name: "Meeting Workbooks", count: 14 },
    { id: 31, name: "Programs", count: 8 },
    { id: 6, name: "Index", count: 6 },
    { id: 17, name: "Guidelines", count: 5 },
    { id: 1, name: "Bible", count: 3 },
  ];

  // ---------- User data ----------

  const TOKEN = /[\p{L}\p{N}\p{M}]+(?:[-‐‑:][\p{L}\p{N}\p{M}]+)*|[^\s\p{L}\p{N}\p{M}­]/gu;
  const SKIP = ".note-dot, .vl, .cl, a.fn, a.xr, .pageNum, .parNum";

  /**
   * Token range of `phrase` inside block `identifier` of `html`, numbered the
   * way ui/src/lib/marks.ts numbers them, so a highlight lands on the words.
   */
  function rangeOf(html, blockType, identifier, phrase) {
    const doc = new DOMParser().parseFromString(`<div id="root">${html}</div>`, "text/html");
    const root = doc.getElementById("root");
    const blocks =
      blockType === 2
        ? [...root.querySelectorAll("span.v[id]")].filter((el) => Number(/^v\d+-\d+-(\d+)-\d+$/.exec(el.id)?.[1]) === identifier)
        : [...root.querySelectorAll(`[id="p${identifier}"]`)];
    const words = [];
    for (const el of blocks) {
      const walker = doc.createTreeWalker(el, NodeFilter.SHOW_TEXT, {
        acceptNode: (n) => (n.parentElement?.closest(SKIP) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
      });
      for (let n = walker.nextNode(); n; n = walker.nextNode()) for (const m of n.data.matchAll(TOKEN)) words.push(m[0]);
    }
    const want = [...phrase.matchAll(TOKEN)].map((m) => m[0]);
    for (let i = 0; i + want.length <= words.length; i++) {
      if (want.every((w, j) => words[i + j] === w)) return { blockType, identifier, start: i, end: i + want.length - 1 };
    }
    missing.push(`highlight not found: ${phrase}`);
    return null;
  }

  const pageKey = (t) => {
    const k = t.kind;
    if ("chapter" in k) return `${t.publication}|c${k.chapter.book}:${k.chapter.chapter}`;
    if ("document" in k) return `${t.publication}|d${k.document}`;
    return `${t.publication}|t${k.dated.date}`;
  };
  const K = {
    lantern10: "lbn_E|c2:10",
    plain10: "prb_E|c2:10",
    week: "mwb_E_202610|d101",
    daily: "mlc26_E|t20261007",
  };

  // Highlights, all six colors: `[guid, color, blockType, identifier, phrase]`.
  const MARK_SPECS = {
    [K.lantern10]: [
      ["m-l3", 1, 2, 3, "the small oil that is poured out in secret"],
      ["m-l4", 2, 2, 4, "Carry the lamp ahead of you"],
      ["m-l9", 3, 2, 9, "Those who mend the nets of others"],
      ["m-l10", 4, 2, 10, "the wall was raised one stone at a time"],
      ["m-l12", 5, 2, 12, "A quiet word at dusk"],
      ["m-l13", 6, 2, 13, "Keep the lamp trimmed, the door unlatched"],
    ],
    [K.week]: [
      ["m-w4", 1, 1, 4, "Learning to Read the Weather of a Conversation"],
      ["m-w6", 2, 1, 6, "Patient listeners notice the mood of a room"],
      ["m-w7", 3, 1, 7, "A question asked at the right moment can open a door"],
      ["m-w8a", 4, 1, 8, "a pause is often the most generous thing"],
      ["m-w8b", 6, 1, 8, "Short sentences travel well"],
      ["m-w9", 5, 1, 9, "Which of my habits help other people feel heard?"],
    ],
    [K.daily]: [["m-d2", 1, 1, 2, "the difference between a lamp held behind and a lamp held ahead"]],
  };

  const marks = {}; // page key -> Mark[] (resolved lazily from the specs)
  function marksOf(key, html) {
    if (!marks[key]) {
      marks[key] = (MARK_SPECS[key] ?? []).flatMap(([guid, color, type, id, phrase]) => {
        const r = rangeOf(html, type, id, phrase);
        return r ? [{ guid, color, ranges: [r] }] : [];
      });
    }
    return marks[key];
  }

  const loc = (keySymbol, extra) => ({ keySymbol, mepsLanguage: 0, issueTag: 0, documentId: null, book: null, chapter: null, ...extra });
  const LOCS = {
    lantern10: { ...loc("lbn", { book: 2, chapter: 10 }), title: "Lanterns 10" },
    tides4: { ...loc("lbn", { book: 1, chapter: 4 }), title: "Tides 4" },
    week: { ...loc("gwb", { issueTag: 202610, documentId: 101 }), title: "October 5-11" },
    daily: { ...loc("mlc", { documentId: 20261007 }), title: "Wednesday, October 7" },
  };

  const notes = [
    { guid: "n1", title: "Patient listening", content: "Try this at the next family dinner: ask one question, then count to five before answering.", blockType: 1, blockIdentifier: 7, markGuid: "m-w7", color: 3, tags: ["Family", "Practice"], lastModified: "2026-10-06T19:10:00Z", location: LOCS.week, page: K.week },
    { guid: "n2", title: "The wall, one stone", content: "Start with the first stone: clear the shelf in the hall on Saturday morning.", blockType: 2, blockIdentifier: 10, markGuid: "m-l10", color: 4, tags: ["Idea", "Reminder"], lastModified: "2026-10-05T07:30:00Z", location: LOCS.lantern10, page: K.lantern10 },
    { guid: "n3", title: "Carry the lamp", content: "Compare with the Plain Rendering: “take the lamp in front of you as you walk”.", blockType: 2, blockIdentifier: 4, markGuid: "m-l4", color: 2, tags: ["Study"], lastModified: "2026-10-04T20:15:00Z", location: LOCS.lantern10, page: K.lantern10 },
    { guid: "n4", title: "Dusk visits", content: "Bring tea to the neighbour on the second floor after the evening news.", blockType: 1, blockIdentifier: 2, markGuid: "m-d2", color: 1, tags: ["Family"], lastModified: "2026-10-07T06:45:00Z", location: LOCS.daily, page: K.daily },
    { guid: "n5", title: "Rehearsal tip", content: "Record the practice talk on the phone and listen to it once without stopping.", blockType: 1, blockIdentifier: 14, markGuid: null, color: null, tags: ["Practice"], lastModified: "2026-10-03T18:00:00Z", location: LOCS.week, page: K.week },
    { guid: "n6", title: "Questions for Sunday", content: "Ask about the new route to the harbour hall and whether the car park is open.", blockType: 0, blockIdentifier: null, markGuid: null, color: null, tags: ["Reminder"], lastModified: "2026-09-28T12:00:00Z", location: null, page: null },
    { guid: "n7", title: "Weekend reading", content: "Finish Tides 4 and note any verses that mention the ferry crossing.", blockType: 0, blockIdentifier: null, markGuid: null, color: null, tags: ["Study"], lastModified: "2026-09-21T09:20:00Z", location: LOCS.tides4, page: null },
    { guid: "n8", title: "Housewarming gift", content: "An oil lamp for the neighbours, and a small tin of lamp oil to go with it.", blockType: 0, blockIdentifier: null, markGuid: null, color: null, tags: ["Family", "Idea"], lastModified: "2026-08-14T15:00:00Z", location: null, page: null },
  ];

  const BOOKMARKS = [
    { slot: 1, title: "Lanterns 10", snippet: "A lamp that is lit in the morning has little to prove…", blockType: 2, blockIdentifier: 1, location: LOCS.lantern10 },
    { slot: 2, title: "Tides 4", snippet: "Share what you have while it is still warm", blockType: 2, blockIdentifier: 2, location: LOCS.tides4 },
    { slot: 3, title: "October 5-11", snippet: "Learning to Read the Weather of a Conversation", blockType: 1, blockIdentifier: 4, location: LOCS.week },
    { slot: 4, title: "Wednesday, October 7", snippet: null, blockType: 0, blockIdentifier: null, location: LOCS.daily },
  ];

  const tagList = () => {
    const names = [...new Set(notes.flatMap((n) => n.tags))].sort();
    return names.map((name, i) => ({ id: i + 1, name, notes: notes.filter((n) => n.tags.includes(name)).length }));
  };

  // ---------- Pages ----------

  function pageOf(target) {
    const k = target.kind;
    const p = pub(target.publication);
    if (!p) throw `unknown publication ${target.publication}`;
    if ("chapter" in k) {
      const { book, chapter, verse } = k.chapter;
      return {
        title: `${bookTitle(book)} ${chapter}`,
        html: chapterHtml(target.publication, book, chapter),
        fragment: verse > 1 ? `v${book}-${chapter}-${verse}-1` : null,
        name: `${bookTitle(book)}-${chapter}`,
      };
    }
    if ("dated" in k) return { title: "", html: dailyHtml(k.dated.date), fragment: null, name: "daily" };
    const id = k.document;
    if (id >= 101 && id <= 104) {
      const label = WEEKS.find((w) => w.id === id).label;
      return { title: label, html: WORKBOOK_HTML(label), fragment: null, name: label };
    }
    if (id === 201) return { title: "Why Small Kindnesses Outlast Grand Gestures", html: STUDY_HTML, fragment: null, name: "study" };
    return { title: "Appendix", html: `<h1 id="p1">Appendix</h1><p id="p2">A short reference page with nothing important in it.</p>`, fragment: null, name: "appendix" };
  }

  function datedPage(kind, date) {
    const n = dateNum(date);
    if (kind === "dailyText") {
      if (n < 20260101 || n > 20261231) return null;
      return { target: { publication: "mlc26_E", kind: { dated: { date: n } } }, title: "", start: date, end: date, html: dailyHtml(n), image: null };
    }
    if (n < 20261005 || n > 20261101) return null;
    if (kind === "workbook") {
      const w = WEEKS[Math.min(3, Math.floor((new Date(`${date}T12:00:00`) - new Date("2026-10-05T12:00:00")) / (7 * 86_400_000)))];
      const target = { publication: "mwb_E_202610", kind: { document: w.id } };
      return { target, title: w.label, start: "2026-10-05", end: "2026-11-01", html: pageOf(target).html, image: pub("mwb_E_202610").cover };
    }
    const target = { publication: "lsq_E_202610", kind: { document: 201 } };
    return { target, title: pageOf(target).title, start: "2026-10-05", end: "2026-11-01", html: STUDY_HTML, image: pub("lsq_E_202610").cover };
  }

  function userDataFor(target) {
    const key = pageKey(target);
    const html = pageOf(target).html;
    return { marks: marksOf(key, html), notes: notes.filter((n) => n.page === key), answers: {} };
  }

  function linkAction(current, href) {
    const m = /^pergament:\/\/bible\/(\d+):(\d+):(\d+)/.exec(href);
    if (m) {
      const dir = current && pub(current)?.isBible ? current : "lbn_E";
      return { kind: "open", target: { publication: dir, kind: { chapter: { book: +m[1], chapter: +m[2], verse: +m[3] } } }, studyNote: false };
    }
    return { kind: "ignore" };
  }

  // ---------- Command table ----------

  const handlers = {
    "plugin:app|version": () => "1.5.0",
    "plugin:event|listen": () => 1,
    "plugin:event|unlisten": () => null,

    list_publications: () => PUBS,
    publication: ({ dir }) => publicationDetail(dir),
    render_page: ({ target }) => pageOf(target),
    link_action: ({ current, href }) => linkAction(current, href),
    open_external: () => null,
    media_links: () => [],
    media_category: ({ key }) => {
      const sub = (list) => list.map(([k, name]) => ({ key: k, name, container: true, image: null, subcategories: [], media: [] }));
      const roots = {
        VideoOnDemand: sub([["VODStudio", "Studio Features"], ["VODMovies", "Films"], ["VODSeries", "Series"], ["VODMinistry", "Ministry Shorts"], ["VODChildren", "For Children"], ["VODTeenagers", "For Teenagers"], ["VODFamily", "For Families"], ["VODProgramsEvents", "Gatherings and Events"]]),
        Audio: sub([["AudioOriginalSongs", "Original Songs"], ["AudioDrama", "Dramatized Readings"], ["AudioPubs", "Audio Editions"], ["AudioMeetings", "Meeting Recordings"]]),
      };
      return { key, name: "", container: true, image: null, subcategories: roots[key] ?? [], media: [] };
    },
    download_media: () => null,
    media_downloads: () => [],
    remove_media: () => null,
    check_updates: () => [],
    year_text: () => ({ text: "“Carry the lamp ahead of you.”", reference: "Lanterns 10:4" }),
    open_display: () => null,
    close_display: () => null,
    remove_publication: () => null,
    import_files: () => [],
    languages: () => [
      { code: "E", name: "English", vernacular: "English", rtl: false, signLanguage: false },
      { code: "X", name: "German", vernacular: "Deutsch", rtl: false, signLanguage: false },
      { code: "S", name: "Spanish", vernacular: "Español", rtl: false, signLanguage: false },
      { code: "F", name: "French", vernacular: "Français", rtl: false, signLanguage: false },
      { code: "I", name: "Italian", vernacular: "Italiano", rtl: false, signLanguage: false },
    ],
    catalog_search: () => [],
    catalog_languages: () => [],
    favorite_entries: () => [],
    chapter_study: ({ dir, book, chapter }) =>
      dir === "lbn_E" && book === 2 && chapter === 10 ? L10_STUDY : { outlineTitle: null, outline: [], verses: [] },
    catalog_cached: () => true,
    load_catalog: () => null,
    home_lists: () => ({ teachingToolbox: TOOLBOX, whatsNew: WHATS_NEW, dailyText: LOCAL_ENTRIES.mlc26_E }),
    categories: () => CATEGORIES,
    category: ({ id }) => ALL_CATALOG.filter((e) => e.item.publication_type === id),
    meetings: () => ({
      workbook: { entry: LOCAL_ENTRIES.mwb_E_202610, start: "2026-10-05", end: "2026-11-01" },
      study: { entry: LOCAL_ENTRIES.lsq_E_202610, start: "2026-10-05", end: "2026-11-01" },
      other: MORE_STUDY,
    }),
    download_publication: () => "x",
    dated_page: ({ kind, date }) => datedPage(kind, date),
    missing_entry: () => null,
    page_user_data: ({ target }) => userDataFor(target),
    save_answer: () => null,
    add_mark: ({ target, color, ranges }) => {
      const guid = `m-new-${Date.now()}`;
      marksOf(pageKey(target), pageOf(target).html).push({ guid, color, ranges });
      return guid;
    },
    set_mark_color: ({ guid, color }) => {
      for (const list of Object.values(marks)) for (const m of list) if (m.guid === guid) m.color = color;
      return null;
    },
    delete_mark: ({ guid }) => {
      for (const key of Object.keys(marks)) marks[key] = marks[key].filter((m) => m.guid !== guid);
      return null;
    },
    save_note: () => "n-new",
    delete_note: ({ guid }) => {
      const i = notes.findIndex((n) => n.guid === guid);
      if (i >= 0) notes.splice(i, 1);
      return null;
    },
    all_notes: ({ tag }) => {
      const name = tag == null ? null : tagList().find((t) => t.id === tag)?.name;
      return notes.filter((n) => !name || n.tags.includes(name)).sort((a, b) => b.lastModified.localeCompare(a.lastModified));
    },
    tags: () => tagList(),
    bookmarks: () => BOOKMARKS,
    user_data_summary: () => ({
      marks: Object.values(MARK_SPECS).reduce((n, l) => n + l.length, 0),
      notes: notes.length,
      tags: tagList().length,
      bookmarks: BOOKMARKS.length,
      device: "Studio Laptop",
    }),
    open_location: ({ loc: l }) => {
      const dir = { lbn: "lbn_E", gwb: "mwb_E_202610", mlc: "mlc26_E" }[l.keySymbol];
      if (!dir) return null;
      if (l.book != null) return { publication: dir, kind: { chapter: { book: l.book, chapter: l.chapter ?? 1, verse: 1 } } };
      if (dir === "mlc26_E") return { publication: dir, kind: { dated: { date: l.documentId ?? 20261007 } } };
      return { publication: dir, kind: { document: l.documentId ?? 101 } };
    },
    export_backup: () => null,
    restore_backup: () => handlers.user_data_summary(),
    media: () => new ArrayBuffer(0),
    research_verses: ({ dir, book, chapter }) => (dir === "lbn_E" && book === 2 && chapter === 10 ? Object.keys(RESEARCH).map(Number) : []),
    research_guide: ({ book, chapter, verse }) => (book === 2 && chapter === 10 ? (RESEARCH[verse] ?? []) : []),
  };

  // ---------- Tauri runtime ----------

  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
    transformCallback(cb) {
      const id = Math.floor(Math.random() * 0xffffffff);
      window[`_${id}`] = cb;
      return id;
    },
    unregisterCallback(id) {
      delete window[`_${id}`];
    },
    convertFileSrc: (path) => path,
    async invoke(cmd, args) {
      const h = handlers[cmd];
      if (!h) {
        missing.push(cmd);
        return null;
      }
      return h(args ?? {});
    },
  };
})();
