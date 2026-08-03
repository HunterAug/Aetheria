import { useEffect, useState } from "react";
import Editor from "./components/Editor";
import ReaderFeed from "./components/ReaderFeed";
import Following from "./components/Following";
import SubscriberPortal from "./components/SubscriberPortal";
import Subscriptions from "./components/Subscriptions";
import PublisherProfileView from "./components/PublisherProfileView";
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
  | "latest"
  | "following"
  | "subscribers"
  | "subscriptions"
  | "about"
  | "profile"
  | "settings";

export default function App() {
  const [tab, setTab] = useState<Tab>("feed");
  // Set whenever an author's name is clicked in any feed - takes over the
  // main pane (like OpenedPostView does locally within a feed) until
  // dismissed, independent of `tab`.
  const [viewingAuthor, setViewingAuthor] = useState<string | null>(null);
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
      <Sidebar
        tab={tab}
        onChange={(t) => {
          setViewingAuthor(null);
          setTab(t);
        }}
      />

      <main className="flex-1 min-w-0 border-x border-ink-800">
        {viewingAuthor ? (
          <PublisherProfileView
            authorPubkey={viewingAuthor}
            onBack={() => setViewingAuthor(null)}
          />
        ) : (
          <>
            {tab === "editor" && <Editor />}
            {tab === "feed" && (
              <ReaderFeed
                title="Home"
                fetchItems={() => delegate.getFollowingFeed()}
                emptyMessage="No posts yet — follow a publisher from the Following tab to see their posts here."
                onOpenProfile={() => setTab("profile")}
                onViewAuthor={setViewingAuthor}
              />
            )}
            {tab === "latest" && (
              <ReaderFeed
                title="Latest"
                fetchItems={() => delegate.getLatestFeed()}
                emptyMessage="No posts yet — be the first to publish one from the Draft tab."
                onOpenProfile={() => setTab("profile")}
                onViewAuthor={setViewingAuthor}
              />
            )}
            {tab === "following" && <Following onViewAuthor={setViewingAuthor} />}
            {tab === "subscribers" && <SubscriberPortal />}
            {tab === "subscriptions" && <Subscriptions />}
            {tab === "about" && <About />}
            {tab === "profile" && (
              <Profile onEditProfile={() => setTab("settings")} />
            )}
            {tab === "settings" && <Settings />}
          </>
        )}
      </main>

      <RightRail />

      {needsName && (
        <FirstRunNamePrompt onDone={() => setNeedsName(false)} />
      )}
    </div>
  );
}
