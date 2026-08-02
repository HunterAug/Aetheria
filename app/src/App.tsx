import { useState } from "react";
import Editor from "./components/Editor";
import ReaderFeed from "./components/ReaderFeed";
import SubscriberPortal from "./components/SubscriberPortal";

type Tab = "editor" | "feed" | "subscribers";

const TABS: { id: Tab; label: string }[] = [
  { id: "editor", label: "Draft" },
  { id: "feed", label: "Feed" },
  { id: "subscribers", label: "Subscribers" },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("feed");

  return (
    <div className="min-h-screen bg-neutral-50 text-neutral-900">
      <header className="border-b border-neutral-200 px-6 py-4 flex items-center justify-between">
        <h1 className="text-lg font-bold">Aetheria</h1>
        <nav className="flex gap-2">
          {TABS.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={`px-3 py-1.5 rounded text-sm font-medium ${
                tab === t.id
                  ? "bg-neutral-900 text-white"
                  : "text-neutral-600 hover:bg-neutral-100"
              }`}
            >
              {t.label}
            </button>
          ))}
        </nav>
      </header>

      <main>
        {tab === "editor" && <Editor />}
        {tab === "feed" && <ReaderFeed />}
        {tab === "subscribers" && <SubscriberPortal />}
      </main>
    </div>
  );
}
