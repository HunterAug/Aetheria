import { useState } from "react";
import Editor from "./components/Editor";
import ReaderFeed from "./components/ReaderFeed";
import SubscriberPortal from "./components/SubscriberPortal";
import About from "./components/About";
import Sidebar from "./components/Sidebar";
import RightRail from "./components/RightRail";

export type Tab = "editor" | "feed" | "subscribers" | "about";

export default function App() {
  const [tab, setTab] = useState<Tab>("feed");

  return (
    <div className="min-h-screen bg-ink-950 text-neutral-200 flex justify-center">
      <div className="flex w-full max-w-6xl">
        <Sidebar tab={tab} onChange={setTab} />

        <main className="flex-1 min-w-0 border-x border-ink-800">
          {tab === "editor" && <Editor />}
          {tab === "feed" && <ReaderFeed />}
          {tab === "subscribers" && <SubscriberPortal />}
          {tab === "about" && <About />}
        </main>

        <RightRail />
      </div>
    </div>
  );
}
