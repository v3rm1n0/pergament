import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { AppProvider, Rail, TitleStrip, useApp } from "./app";
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
      <TitleStrip />
      <div className="flex min-h-0 flex-1">
        <Rail />
        <main className="relative flex min-w-0 flex-1 flex-col bg-bg">
          <div className="flex min-h-0 flex-1 flex-col">
            <CurrentView />
          </div>
          <PlayerHost />
          <ImageViewer />
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
