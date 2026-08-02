import { SearchIcon } from "./icons";

export default function RightRail() {
  return (
    <aside className="w-72 shrink-0 hidden lg:block px-5 py-5 space-y-4">
      <div className="flex items-center gap-2 rounded-full bg-ink-900 border border-ink-700 px-3.5 py-2 text-sm text-neutral-500">
        <SearchIcon className="w-4 h-4" />
        <span>Search Aetheria</span>
      </div>

      <div className="rounded-xl border border-ink-800 bg-ink-900 p-5">
        <img src="/logo.png" alt="" className="w-9 h-9 mb-3" />
        <h3 className="text-neutral-100 font-semibold mb-1">
          Sovereign by design
        </h3>
        <p className="text-sm text-neutral-400 leading-relaxed">
          Every post here is signed by your own keys and stored on Freenet —
          no publisher account, no platform that can pull it down.
        </p>
      </div>

      <div className="rounded-xl border border-ink-800 bg-ink-900 p-5 text-sm text-neutral-400 leading-relaxed">
        <h3 className="text-neutral-100 font-semibold mb-1">
          Local Delegate
        </h3>
        Reads and writes go through your local Delegate daemon on{" "}
        <code className="text-aecyan-400">127.0.0.1:47021</code>. Nothing
        leaves this machine except signed contract state.
      </div>
    </aside>
  );
}
