import { useEffect, useState } from "react";
import { delegate, type SubscriberEntry, type SubscriptionInfo } from "../lib/delegate";

function short(hex: string): string {
  return hex.length > 16 ? `${hex.slice(0, 8)}…${hex.slice(-6)}` : hex;
}

/// Publisher-side view: your subscription tiers (read-only - no editing UI
/// yet, see CLAUDE.md's "Known stub" section), your subscribers, and your
/// publication key. Connecting a wallet and paying for a subscription is a
/// reader action and lives in `Subscriptions.tsx` instead.
export default function SubscriberPortal() {
  const [info, setInfo] = useState<SubscriptionInfo | null>(null);
  const [subscribers, setSubscribers] = useState<SubscriberEntry[] | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([delegate.getSubscriptionInfo(), delegate.listSubscribers()])
      .then(([subInfo, subs]) => {
        setInfo(subInfo);
        setSubscribers(subs);
      })
      .catch((err) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setLoaded(true));
  }, []);

  return (
    <div className="px-6 py-5 max-w-2xl mx-auto">
      <h2 className="text-xl font-semibold text-neutral-100 mb-1">Subscribers</h2>
      <p className="text-sm text-neutral-500 mb-5">
        People who pay to unlock your subscriber-only posts, delivered via an ECDH-encrypted
        epoch key bundle (design doc §5.2, Workflow B).
      </p>

      {!loaded ? (
        <p className="text-sm text-neutral-500">Loading…</p>
      ) : (
        <div className="space-y-6">
          {error && <p className="text-sm text-red-400">{error}</p>}

          <section className="space-y-2">
            <h3 className="text-sm font-semibold text-neutral-300">Subscription tiers</h3>
            <div className="space-y-2">
              {info?.tiers.map((tier) => (
                <div
                  key={tier.tier_id}
                  className="rounded-lg border border-ink-700 bg-ink-900 p-3"
                >
                  <p className="text-sm font-medium text-neutral-200">{tier.name}</p>
                  <p className="text-xs text-neutral-500">
                    {tier.price_sats_per_month.toLocaleString()} sats/month
                    {tier.features.length > 0 ? ` (${tier.features.join(", ")})` : ""}
                  </p>
                </div>
              ))}
            </div>
          </section>

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
