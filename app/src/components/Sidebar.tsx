import { EditIcon, HomeIcon, InfoIcon, PlusIcon, UsersIcon } from "./icons";
import type { Tab } from "../App";

const NAV: { id: Tab; label: string; icon: (p: { className?: string }) => JSX.Element }[] = [
  { id: "feed", label: "Home", icon: HomeIcon },
  { id: "editor", label: "Draft", icon: EditIcon },
  { id: "subscribers", label: "Subscribers", icon: UsersIcon },
  { id: "about", label: "About", icon: InfoIcon },
];

export default function Sidebar({
  tab,
  onChange,
}: {
  tab: Tab;
  onChange: (t: Tab) => void;
}) {
  return (
    <aside className="w-60 shrink-0 h-screen sticky top-0 flex flex-col border-r border-ink-800 bg-ink-900/60">
      <div className="flex items-center gap-2 px-5 py-5">
        <img src="/logo.png" alt="" className="w-8 h-8" />
        <span className="text-lg font-semibold tracking-tight text-neutral-100">
          Aetheria
        </span>
      </div>

      <nav className="flex-1 px-3 space-y-0.5">
        {NAV.map(({ id, label, icon: Icon }) => {
          const active = tab === id;
          return (
            <button
              key={id}
              onClick={() => onChange(id)}
              className={`w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                active
                  ? "bg-ink-800 text-white"
                  : "text-neutral-400 hover:bg-ink-850 hover:text-neutral-100"
              }`}
            >
              <Icon className={active ? "text-aeblue-400" : ""} />
              {label}
            </button>
          );
        })}
      </nav>

      <div className="p-3">
        <button
          onClick={() => onChange("editor")}
          className="w-full flex items-center justify-center gap-2 rounded-lg bg-aetheria-gradient text-white text-sm font-semibold py-2.5 shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition"
        >
          <PlusIcon className="w-4 h-4" />
          New post
        </button>
      </div>
    </aside>
  );
}
