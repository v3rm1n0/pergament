import { StrictMode, useLayoutEffect } from "react";
import { createRoot } from "react-dom/client";
import { AppProvider, Rail, TabBar, TitleStrip, useApp } from "./app";
import { GoTo } from "./components/go-to";
import { ImageViewer } from "./components/image-viewer";
import { PlayerHost } from "./components/player-host";
import { isDisplayWindow } from "./lib/display";
import { isNoteWindow } from "./lib/note-window";
import { NoteWindowView } from "./views/note-window";
import { DisplayView } from "./views/display";
import { ChaptersView } from "./views/chapters";
import { HomeView } from "./views/home";
import { BibleView } from "./views/bible";
import { CategoryView, LibraryView } from "./views/library";
import { MediaCategoryView } from "./views/media";
import { MeetingsView } from "./views/meetings";
import { OnlineView } from "./views/online";
import { PersonalView } from "./views/personal";
import { PublicationView } from "./views/publication";
import { ReaderView } from "./views/reader";
import { SettingsView } from "./views/settings";
import { t } from "./lib/i18n";
import { installFocusModality } from "./lib/focus";
import { watchDesignClasses } from "./lib/design-classes";
import { restoreScroll, scroller } from "./lib/scroll";
import { useCompact } from "./lib/viewport";
import "./styles.css";

function CurrentView() {
  const { view } = useApp();
  // Coming back to a section, a view continues where it was left. The reader does that itself once its text is there.
  useLayoutEffect(() => {
    const el = scroller();
    if (view.scroll && view.name !== "reader" && el) restoreScroll(el, view.scroll);
  }, [view]);
  switch (view.name) {
    case "home":
      return <HomeView />;
    case "bible":
      return <BibleView />;
    case "publication":
      return <PublicationView key={view.dir} dir={view.dir} tab={view.tab} />;
    case "chapters":
      return <ChaptersView dir={view.dir} book={view.book} />;
    case "reader":
      return <ReaderView target={view.target} note={view.note} noteGuid={view.noteGuid} scroll={view.scroll} />;
    case "online":
      return <OnlineView />;
    case "settings":
      return <SettingsView />;
    case "library":
      return <LibraryView tab={view.tab} />;
    case "category":
      return <CategoryView key={view.id} id={view.id} title={view.title} />;
    case "media":
      return <MediaCategoryView key={view.key} catKey={view.key} title={view.title} />;
    case "meetings":
      return <MeetingsView week={view.week} />;
    case "personal":
      return <PersonalView />;
  }
}

function Shell() {
  const { back } = useApp();
  // Narrow windows (phones) get a tab bar along the bottom instead of the rail, and clear the notch and the home bar.
  const compact = useCompact();
  return (
    <div
      className="flex h-full flex-col pt-[env(safe-area-inset-top)]"
      onMouseUp={(e) => {
        // Mouse "back" button.
        if (e.button === 3) back();
      }}
    >
      <a
        href="#content"
        onClick={(e) => {
          e.preventDefault();
          document.getElementById("content")?.focus();
        }}
        className="sr-only focus:not-sr-only focus:absolute focus:left-2 focus:top-2 focus:z-50 focus:rounded-md focus:border focus:border-line focus:bg-surface focus:px-3 focus:py-1.5 focus:text-sm"
      >
        {t("Skip to content")}
      </a>
      {!compact && <TitleStrip />}
      <div className="flex min-h-0 flex-1">
        {!compact && <Rail />}
        <main id="content" tabIndex={-1} className="relative flex min-w-0 flex-1 flex-col bg-bg outline-none">
          <div className="flex min-h-0 flex-1 flex-col">
            <CurrentView />
          </div>
          <PlayerHost />
          <ImageViewer />
          <GoTo />
        </main>
      </div>
      {compact && <TabBar />}
    </div>
  );
}

installFocusModality();
watchDesignClasses();

// The second display and the note window are the same page in other windows; each shows only what the app sends.
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {isDisplayWindow() ? (
      <DisplayView />
    ) : isNoteWindow() ? (
      <NoteWindowView />
    ) : (
      <AppProvider>
        <Shell />
      </AppProvider>
    )}
  </StrictMode>,
);
