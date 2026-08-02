import { useState } from "react";
import Editor from "./components/Editor";
import ReaderFeed from "./components/ReaderFeed";
import SubscriberPortal from "./components/SubscriberPortal";
import About from "./components/About";
import Profile from "./components/Profile";
import Sidebar from "./components/Sidebar";
import RightRail from "./components/RightRail";

export type Tab = "editor" | "feed" | "subscribers" | "about" | "profile";

export default function App() {
  const [tab, setTab] = useState<Tab>("feed");

  return (
    <div className="min-h-screen bg-ink-950 text-neutral-200 flex">
      <Sidebar tab={tab} onChange={setTab} />

      <main className="flex-1 min-w-0 border-x border-ink-800">
        {tab === "editor" && <Editor />}
        {tab === "feed" && <ReaderFeed onOpenProfile={() => setTab("profile")} />}
        {tab === "subscribers" && <SubscriberPortal />}
        {tab === "about" && <About />}
        {tab === "profile" && <Profile />}
      </main>

      <RightRail />
    </div>
  );
}
