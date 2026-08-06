import { useEffect, useState } from "react";
import Editor from "./components/Editor";
import ReaderFeed from "./components/ReaderFeed";
import Following from "./components/Following";
import PublisherProfileView from "./components/PublisherProfileView";
import About from "./components/About";
import Profile from "./components/Profile";
import Settings from "./components/Settings";
import Sidebar from "./components/Sidebar";
import RightRail from "./components/RightRail";
import FirstRunNamePrompt from "./components/FirstRunNamePrompt";
import UnlockScreen from "./components/UnlockScreen";
import OpenedPostView from "./components/OpenedPostView";
import { delegate, type LockStatus, type OpenedPost } from "./lib/delegate";
import { startNewPostNotifications } from "./lib/notifications";

export type Tab =
  | "editor"
  | "feed"
  | "latest"
  | "following"
  | "about"
  | "profile"
  | "settings";

export default function App() {
  const [tab, setTab] = useState<Tab>("feed");
  // Set whenever an author's name is clicked in any feed - takes over the
  // main pane (like OpenedPostView does locally within a feed) until
  // dismissed, independent of `tab`.
  const [viewingAuthor, setViewingAuthor] = useState<string | null>(null);
  // Set when a post is opened from the search bar (RightRail.tsx), which
  // isn't scoped to any particular feed/tab - takes precedence over
  // `viewingAuthor` in the render below.
  const [searchOpenedPost, setSearchOpenedPost] = useState<OpenedPost | null>(null);
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

  // Desktop notifications for new posts from publishers you follow. Only
  // once unlocked: the delegate has no followed list (and no identity at
  // all) before that, so there's nothing to be notified about. Runs for as
  // long as the app does - including while the window is hidden in the
  // system tray, which is what makes it worth having.
  useEffect(() => {
    if (!lockStatus || lockStatus.locked) return;
    return startNewPostNotifications();
  }, [lockStatus]);

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
          setSearchOpenedPost(null);
          setTab(t);
        }}
      />

      <main className="flex-1 min-w-0 border-x border-ink-800">
        {searchOpenedPost ? (
          <OpenedPostView
            post={searchOpenedPost}
            onBack={() => setSearchOpenedPost(null)}
            onOpenProfile={() => {
              setSearchOpenedPost(null);
              setTab("profile");
            }}
            onViewAuthor={(pubkey) => {
              setSearchOpenedPost(null);
              setViewingAuthor(pubkey);
            }}
          />
        ) : viewingAuthor ? (
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
                emptyMessage="No posts yet. Follow a publisher from the Following tab to see their posts here."
                onOpenProfile={() => setTab("profile")}
                onViewAuthor={setViewingAuthor}
              />
            )}
            {tab === "latest" && (
              <ReaderFeed
                title="Latest"
                fetchItems={() => delegate.getLatestFeed()}
                emptyMessage="No posts yet. Be the first to publish one from the Draft tab."
                onOpenProfile={() => setTab("profile")}
                onViewAuthor={setViewingAuthor}
              />
            )}
            {tab === "following" && <Following onViewAuthor={setViewingAuthor} />}
            {tab === "about" && <About />}
            {tab === "profile" && (
              <Profile onEditProfile={() => setTab("settings")} />
            )}
            {tab === "settings" && <Settings />}
          </>
        )}
      </main>

      <RightRail
        onOpenPost={setSearchOpenedPost}
        onViewAuthor={(pubkey) => {
          setSearchOpenedPost(null);
          setViewingAuthor(pubkey);
        }}
      />

      {needsName && (
        <FirstRunNamePrompt onDone={() => setNeedsName(false)} />
      )}
    </div>
  );
}
