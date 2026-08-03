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
import UnlockScreen from "./components/UnlockScreen";
import { delegate, type LockStatus } from "./lib/delegate";

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
  // `null` = not checked yet, so neither the unlock screen nor the app
  // chrome flashes on for an instant before the delegate answers.
  const [lockStatus, setLockStatus] = useState<LockStatus | null>(null);
  const [needsName, setNeedsName] = useState<boolean | null>(null);

  useEffect(() => {
    delegate
      .lockStatus()
      .then(setLockStatus)
      // Delegate unreachable - nothing else will work either, but let the
      // rest of the app render and surface that through its own error
      // states rather than getting stuck on a blank screen forever.
      .catch(() => setLockStatus({ locked: false, has_existing_identity: true }));
  }, []);

  useEffect(() => {
    if (!lockStatus || lockStatus.locked) return;
    delegate
      .getProfile()
      .then((p) => setNeedsName(p.display_name.trim() === ""))
      .catch(() => setNeedsName(false));
  }, [lockStatus]);

  if (!lockStatus) {
    return <div className="min-h-screen bg-ink-950" />;
  }

  if (lockStatus.locked) {
    return (
      <UnlockScreen
        hasExistingIdentity={lockStatus.has_existing_identity}
        onUnlocked={() => setLockStatus({ locked: false, has_existing_identity: true })}
      />
    );
  }

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
