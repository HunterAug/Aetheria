import Link from "next/link";
import { isReleased } from "@/lib/config";

const FEATURES = [
  {
    title: "Once they have it, it's theirs to keep",
    body: "Freenet delivers your posts straight to your readers, no server in between. Once someone receives a post, it's saved on their own device — readable offline, permanently. No update, algorithm change, or shutdown can reach into their library and take it back.",
  },
  {
    title: "No account for anyone to suspend",
    body: "Your writing lives on Freenet, a peer-to-peer network with no central server. There's no company account to suspend and no single computer to unplug to make your publication disappear — whether the reason is a policy dispute or something more serious.",
  },
  {
    title: "You hold the keys",
    body: "Your identity is a cryptographic keypair on your own machine, protected by a passphrase only you know. Every post you publish is signed by it, so readers can always verify it's really from you.",
  },
  {
    title: "Direct, non-custodial payments",
    body: "Subscriptions are paid wallet-to-wallet over the Lightning Network (via Nostr Wallet Connect). Money goes straight from a reader to a publisher, and no platform ever touches it.",
  },
];

export default async function Home() {
  const released = await isReleased();
  return (
    <div>
      <section className="max-w-5xl mx-auto px-6 pt-20 pb-16 text-center">
        <h1 className="text-4xl sm:text-5xl font-bold tracking-tight text-neutral-50">
          Your readers. Your archive.
          <br />
          <span className="aetheria-gradient-text">Nobody else's call.</span>
        </h1>
        <p className="mt-6 text-lg text-neutral-400 max-w-2xl mx-auto leading-relaxed">
          Substack can change its rules. Medium can reshuffle its paywall.
          Any platform can suspend an account over a policy dispute — nothing
          illegal required — and take your archive and your readers with it.
          Aetheria puts no company in the middle: your posts reach the people
          who follow you directly over Freenet, a peer-to-peer network, and
          once a reader has one, it's saved on their device for good. No
          policy change can undeliver it.
        </p>
        <div className="mt-8 flex items-center justify-center gap-4">
          {released ? (
            <Link
              href="/download"
              className="rounded-lg aetheria-gradient text-white text-sm font-semibold px-6 py-3 shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition"
            >
              Download for Windows
            </Link>
          ) : (
            <span
              aria-disabled="true"
              className="rounded-lg border border-ink-700 text-sm font-semibold px-6 py-3 text-neutral-500 cursor-not-allowed select-none"
            >
              Coming soon!
            </span>
          )}
          <Link
            href="/docs"
            className="rounded-lg border border-ink-700 text-sm font-semibold px-6 py-3 text-neutral-300 hover:bg-ink-900 transition"
          >
            Read the docs
          </Link>
        </div>
        <p className="mt-4 text-xs text-neutral-600">
          Free, open source, and it always will be.
        </p>
      </section>

      <section className="border-t border-ink-800 bg-ink-900/40">
        <div className="max-w-5xl mx-auto px-6 py-16 grid sm:grid-cols-2 gap-8">
          {FEATURES.map((f) => (
            <div key={f.title} className="rounded-xl border border-ink-800 bg-ink-900 p-6">
              <h3 className="text-neutral-100 font-semibold mb-2">{f.title}</h3>
              <p className="text-sm text-neutral-400 leading-relaxed">{f.body}</p>
            </div>
          ))}
        </div>
      </section>

      <section className="max-w-5xl mx-auto px-6 py-16 text-center">
        <h2 className="text-2xl font-bold text-neutral-100">
          See what people are actually publishing
        </h2>
        <p className="mt-3 text-neutral-400 max-w-xl mx-auto">
          You don't need to install anything to look. Browse the most recent
          posts published on the real Aetheria network, straight from this
          page.
        </p>
        <div className="mt-6">
          <Link
            href="/latest"
            className="inline-block rounded-lg border border-ink-700 text-sm font-semibold px-6 py-3 text-neutral-300 hover:bg-ink-900 transition"
          >
            Browse latest posts →
          </Link>
        </div>
      </section>
    </div>
  );
}
