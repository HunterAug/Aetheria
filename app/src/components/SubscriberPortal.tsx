import { useEffect, useState } from "react";
import {
  delegate,
  type SubscriberEntry,
  type SubscriptionInfo,
} from "../lib/delegate";

const inputClass =
  "w-full rounded-lg bg-ink-900 border border-ink-700 p-2.5 text-sm text-neutral-200 placeholder:text-neutral-500 focus:outline-none focus:ring-2 focus:ring-aeblue-500/50 focus:border-aeblue-500";

type WalletStatus =
  | { kind: "idle" }
  | { kind: "connecting" }
  | { kind: "error"; message: string };

type SubscribeStatus =
  | { kind: "idle" }
  | { kind: "paying" }
  | { kind: "success"; epochId: number }
  | { kind: "partial"; epochId: number; message: string }
  | { kind: "error"; message: string };

function short(hex: string): string {
  return hex.length > 16 ? `${hex.slice(0, 8)}…${hex.slice(-6)}` : hex;
}

export default function SubscriberPortal() {
  const [info, setInfo] = useState<SubscriptionInfo | null>(null);
  const [subscribers, setSubscribers] = useState<SubscriberEntry[] | null>(null);
  const [walletUri, setWalletUri] = useState("");
  const [walletStatus, setWalletStatus] = useState<WalletStatus>({ kind: "idle" });
  const [subscribeStatus, setSubscribeStatus] = useState<SubscribeStatus>({ kind: "idle" });
  const [loaded, setLoaded] = useState(false);

  async function refresh() {
    const [subInfo, subs] = await Promise.all([
      delegate.getSubscriptionInfo(),
      delegate.listSubscribers(),
    ]);
    setInfo(subInfo);
    setSubscribers(subs);
  }

  useEffect(() => {
    refresh()
      .catch((err) =>
        setWalletStatus({
          kind: "error",
          message: err instanceof Error ? err.message : String(err),
        }),
      )
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

  async function subscribeToTier(tierId: number) {
    setSubscribeStatus({ kind: "paying" });
    try {
      const result = await delegate.subscribe(tierId);
      if (result.network_synced) {
        setSubscribeStatus({ kind: "success", epochId: result.epoch_id });
      } else {
        setSubscribeStatus({
          kind: "partial",
          epochId: result.epoch_id,
          message: `Payment verified and access granted locally, but the key bundle hasn't \
reached the network yet (${result.network_error ?? "unknown error"}). It'll keep retrying.`,
        });
      }
      await refresh();
    } catch (err) {
      setSubscribeStatus({
        kind: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  return (
    <div className="px-6 py-5 max-w-2xl mx-auto">
      <h2 className="text-xl font-semibold text-neutral-100 mb-1">Subscribers</h2>
      <p className="text-sm text-neutral-500 mb-5">
        Connect a Lightning wallet via Nostr Wallet Connect (NIP-47), then subscribe to a
        tier — payment is verified and an epoch key bundle is delivered via ECDH before
        access is granted (design doc §5.2, Workflow B).
      </p>

      {!loaded ? (
        <p className="text-sm text-neutral-500">Loading…</p>
      ) : (
        <div className="space-y-6">
          {/* Wallet connection */}
          <section className="space-y-2">
            <h3 className="text-sm font-semibold text-neutral-300">Wallet</h3>
            {info?.wallet_connected ? (
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

          {/* Tiers */}
          <section className="space-y-2">
            <h3 className="text-sm font-semibold text-neutral-300">Subscription tiers</h3>
            <div className="space-y-2">
              {info?.tiers.map((tier) => (
                <div
                  key={tier.tier_id}
                  className="flex items-center justify-between rounded-lg border border-ink-700 bg-ink-900 p-3"
                >
                  <div>
                    <p className="text-sm font-medium text-neutral-200">{tier.name}</p>
                    <p className="text-xs text-neutral-500">
                      {tier.price_sats_per_month.toLocaleString()} sats/month
                      {tier.features.length > 0 ? ` — ${tier.features.join(", ")}` : ""}
                    </p>
                  </div>
                  <button
                    onClick={() => subscribeToTier(tier.tier_id)}
                    disabled={!info?.wallet_connected || subscribeStatus.kind === "paying"}
                    className="px-4 py-1.5 rounded-lg bg-aetheria-gradient text-white text-sm font-semibold shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition disabled:opacity-40 disabled:cursor-not-allowed whitespace-nowrap"
                  >
                    {subscribeStatus.kind === "paying" ? "Paying…" : "Subscribe"}
                  </button>
                </div>
              ))}
            </div>
            {subscribeStatus.kind === "success" && (
              <p className="text-sm text-aecyan-400">
                Subscribed — epoch {subscribeStatus.epochId} key delivered and synced to the
                network.
              </p>
            )}
            {subscribeStatus.kind === "partial" && (
              <p className="text-sm text-amber-400">{subscribeStatus.message}</p>
            )}
            {subscribeStatus.kind === "error" && (
              <p className="text-sm text-red-400">{subscribeStatus.message}</p>
            )}
          </section>

          {/* Your subscribers (publisher view) */}
          <section className="space-y-2">
            <h3 className="text-sm font-semibold text-neutral-300">Your subscribers</h3>
            {subscribers && subscribers.length > 0 ? (
              <ul className="space-y-1">
                {subscribers.map((s) => (
                  <li
                    key={`${s.subscriber_pubkey}-${s.epoch_id}`}
                    className="flex items-center justify-between text-xs text-neutral-400 rounded-lg border border-ink-800 px-3 py-2"
                  >
                    <span className="font-mono">{short(s.subscriber_pubkey)}</span>
                    <span>epoch {s.epoch_id}</span>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-sm text-neutral-500">No subscribers yet.</p>
            )}
          </section>

          {info && (
            <p className="text-xs text-neutral-600">
              Your publication key: <span className="font-mono">{short(info.publisher_pubkey)}</span>
            </p>
          )}
        </div>
      )}
    </div>
  );
}
