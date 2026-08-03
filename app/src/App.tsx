import { useEffect, useState } from "react";
import Editor from "./components/Editor";
import ReaderFeed from "./components/ReaderFeed";
import Following from "./components/Following";
import SubscriberPortal from "./components/SubscriberPortal";
import About from "./components/About";
import Profile from "./components/Profile";
import Settings from "./components/Settings";
import Sidebar from "./components/Sidebar";
import RightRail from "./components/RightRail";
import FirstRunNamePrompt from "./components/FirstRunNamePrompt";
import { delegate } from "./lib/delegate";

export type Tab =
  | "editor"
  | "feed"
  | "following"
  | "subscribers"
  | "about"
  | "profile"
  | "settings";

export default function App() {
  const [tab, setTab] = useState<Tab>("feed");
  // `null` = not checked yet, so the popup never flashes on for an instant
  // before the profile fetch resolves.
  const [needsName, setNeedsName] = useState<boolean | null>(null);

  useEffect(() => {
    delegate
      .getProfile()
      .then((p) => setNeedsName(p.display_name.trim() === ""))
      .catch(() => setNeedsName(false));
  }, []);

  return (
    <div className="min-h-screen bg-ink-950 text-neutral-200 flex">
      <Sidebar tab={tab} onChange={setTab} />

      <main className="flex-1 min-w-0 border-x border-ink-800">
        {tab === "editor" && <Editor />}
        {tab === "feed" && <ReaderFeed onOpenProfile={() => setTab("profile")} />}
        {tab === "following" && <Following />}
        {tab === "subscribers" && <SubscriberPortal />}
        {tab === "about" && <About />}
        {tab === "profile" && (
          <Profile onEditProfile={() => setTab("settings")} />
        )}
        {tab === "settings" && <Settings />}
      </main>

      <RightRail />

      {needsName && (
        <FirstRunNamePrompt onDone={() => setNeedsName(false)} />
      )}
    </div>
  );
}
