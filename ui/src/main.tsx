import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { AppProvider, Rail, TitleStrip, useApp } from "./app";
import { ChaptersView } from "./views/chapters";
import { HomeView } from "./views/home";
import { CategoryView, LibraryView } from "./views/library";
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
        <main className="flex min-w-0 flex-1 flex-col bg-bg">
          <CurrentView />
        </main>
      </div>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <AppProvider>
      <Shell />
    </AppProvider>
  </StrictMode>,
);
