import { useEffect, useState } from "react";
import { delegate, type FollowedPublisher } from "../lib/delegate";
import { shortHex } from "../lib/format";

const inputClass =
  "w-full rounded-lg bg-ink-900 border border-ink-700 p-2.5 text-sm text-neutral-200 placeholder:text-neutral-500 focus:outline-none focus:ring-2 focus:ring-aeblue-500/50 focus:border-aeblue-500";

type WalletStatus =
  | { kind: "idle" }
  | { kind: "connecting" }
  | { kind: "error"; message: string };

type PerPublisherStatus =
  | { kind: "idle" }
  | { kind: "paying" }
  | { kind: "error"; message: string };

/// Reader-side subscription management - "who am I paying to see their pro
/// content", separate from `SubscriberPortal.tsx`'s publisher-side "who
/// pays me" view. Wallet connection lives here since connecting one is
/// fundamentally a reader action (you connect a wallet in order to pay).
///
/// Every followed publisher gets a "Subscribe" button for real, but it only
/// actually works for this delegate's own identity - which can never appear
/// in the followed list (`follow_publisher` refuses to let you follow
/// yourself). So in practice every button here surfaces the same honest,
/// immediate error rather than a fake success: there's no channel yet for a
/// reader to learn a stranger's secp256k1 key (see CLAUDE.md's "Known stub"
/// section). This is a real, in-scope gap, not a bug - see the design note
/// this component's error message points at.
export default function Subscriptions() {
  const [followed, setFollowed] = useState<FollowedPublisher[] | null>(null);
  const [walletConnected, setWalletConnected] = useState(false);
  const [walletUri, setWalletUri] = useState("");
  const [walletStatus, setWalletStatus] = useState<WalletStatus>({ kind: "idle" });
  const [perPublisher, setPerPublisher] = useState<Record<string, PerPublisherStatus>>({});
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    const [info, list] = await Promise.all([
      delegate.getSubscriptionInfo(),
      delegate.listFollowedPublishers(),
    ]);
    setWalletConnected(info.wallet_connected);
    setFollowed(list);
  }

  useEffect(() => {
    refresh()
      .catch((err) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setLoaded(true));
  }, []);

  async function connectWallet() {
    if (!walletUri.trim()) return;
    setWalletStatus({ kind: "connecting" });
    try {
      await delegate.connectWallet(walletUri.trim());
      setWalletUri("");
      setWalletStatus({ kind: "idle" });
      await refresh();
    } catch (err) {
      setWalletStatus({
        kind: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  async function subscribeTo(authorPubkey: string) {
    setPerPublisher((s) => ({ ...s, [authorPubkey]: { kind: "paying" } }));
    try {
      await delegate.subscribe(0, authorPubkey);
      setPerPublisher((s) => ({ ...s, [authorPubkey]: { kind: "idle" } }));
      await refresh();
    } catch (err) {
      setPerPublisher((s) => ({
        ...s,
        [authorPubkey]: {
          kind: "error",
          message: err instanceof Error ? err.message : String(err),
        },
      }));
    }
  }

  return (
    <div className="px-6 py-5 max-w-2xl mx-auto">
      <h2 className="text-xl font-semibold text-neutral-100 mb-1">Subscriptions</h2>
      <p className="text-sm text-neutral-500 mb-5">
        Pay a publisher you follow to unlock their subscriber-only posts.
      </p>

      {!loaded ? (
        <p className="text-sm text-neutral-500">Loading…</p>
      ) : (
        <div className="space-y-6">
          <section className="space-y-2">
            <h3 className="text-sm font-semibold text-neutral-300">Wallet</h3>
            {walletConnected ? (
              <p className="text-sm text-aecyan-400">Wallet connected.</p>
            ) : (
              <div className="flex gap-2">
                <input
                  className={inputClass}
                  placeholder="nostr+walletconnect://..."
                  value={walletUri}
                  onChange={(e) => setWalletUri(e.target.value)}
                />
                <button
                  onClick={connectWallet}
                  disabled={!walletUri.trim() || walletStatus.kind === "connecting"}
                  className="px-4 py-2 rounded-lg bg-aetheria-gradient text-white text-sm font-semibold shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition disabled:opacity-40 disabled:cursor-not-allowed whitespace-nowrap"
                >
                  {walletStatus.kind === "connecting" ? "Connecting…" : "Connect Wallet"}
                </button>
              </div>
            )}
            {walletStatus.kind === "error" && (
              <p className="text-sm text-red-400">{walletStatus.message}</p>
            )}
          </section>

          <section className="space-y-2">
            <h3 className="text-sm font-semibold text-neutral-300">Publishers you follow</h3>
            {followed?.length === 0 && (
              <p className="text-sm text-neutral-500">
                Follow a publisher from the Following tab to see subscription options here.
              </p>
            )}
            {followed && followed.length > 0 && (
              <div className="space-y-2">
                {followed.map((f) => {
                  const status = perPublisher[f.author_pubkey] ?? { kind: "idle" };
                  return (
                    <div
                      key={f.author_pubkey}
                      className="rounded-lg border border-ink-700 bg-ink-900 p-3"
                    >
                      <div className="flex items-center justify-between gap-3">
                        <div className="min-w-0">
                          <p className="text-sm font-medium text-neutral-200 truncate">
                            {f.display_name}
                          </p>
                          <p className="text-xs text-neutral-500 font-mono">
                            {shortHex(f.author_pubkey)}
                          </p>
                        </div>
                        <button
                          onClick={() => subscribeTo(f.author_pubkey)}
                          disabled={!walletConnected || status.kind === "paying"}
                          className="px-4 py-1.5 rounded-lg bg-aetheria-gradient text-white text-sm font-semibold shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition disabled:opacity-40 disabled:cursor-not-allowed whitespace-nowrap"
                        >
                          {status.kind === "paying" ? "Paying…" : "Subscribe"}
                        </button>
                      </div>
                      {status.kind === "error" && (
                        <p className="text-xs text-red-400 mt-2">{status.message}</p>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
            <p className="text-xs text-neutral-600 mt-2">
              Subscribing to another publisher isn't supported yet - there's no way for your
              delegate to securely learn their encryption key (see CLAUDE.md's Known stub
              section).
            </p>
          </section>

          {error && <p className="text-sm text-red-400">{error}</p>}
        </div>
      )}
    </div>
  );
}
