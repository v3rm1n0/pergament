import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { AppProvider, Rail, TitleStrip, useApp } from "./app";
import { GoTo } from "./components/go-to";
import { ImageViewer } from "./components/image-viewer";
import { PlayerHost } from "./components/player-host";
import { isDisplayWindow } from "./lib/display";
import { DisplayView } from "./views/display";
import { ChaptersView } from "./views/chapters";
import { HomeView } from "./views/home";
import { CategoryView, LibraryView } from "./views/library";
import { MediaCategoryView } from "./views/media";
import { MeetingsView } from "./views/meetings";
import { OnlineView } from "./views/online";
import { PersonalView } from "./views/personal";
import { PublicationView } from "./views/publication";
import { ReaderView } from "./views/reader";
import { SettingsView } from "./views/settings";
import { t } from "./lib/i18n";
import "./styles.css";

function CurrentView() {
  const { view } = useApp();
  switch (view.name) {
    case "home":
      return <HomeView />;
    case "publication":
      return <PublicationView key={view.dir} dir={view.dir} tab={view.tab} />;
    case "chapters":
      return <ChaptersView dir={view.dir} book={view.book} />;
    case "reader":
      return <ReaderView target={view.target} note={view.note} />;
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
      return <MeetingsView />;
    case "personal":
      return <PersonalView />;
  }
}

function Shell() {
  const { back } = useApp();
  return (
    <div
      className="flex h-full flex-col"
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
      <TitleStrip />
      <div className="flex min-h-0 flex-1">
        <Rail />
        <main id="content" tabIndex={-1} className="relative flex min-w-0 flex-1 flex-col bg-bg outline-none">
          <div className="flex min-h-0 flex-1 flex-col">
            <CurrentView />
          </div>
          <PlayerHost />
          <ImageViewer />
          <GoTo />
        </main>
      </div>
    </div>
  );
}

// The second display is the same page in another window; it shows only what the app sends.
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {isDisplayWindow() ? (
      <DisplayView />
    ) : (
      <AppProvider>
        <Shell />
      </AppProvider>
    )}
  </StrictMode>,
);
