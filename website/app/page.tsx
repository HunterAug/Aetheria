import Link from "next/link";
import { isReleased } from "@/lib/config";

const FEATURES = [
  {
    title: "Nobody can take your posts down",
    body: "Your writing is stored on Freenet, a peer-to-peer network with no central server. There's no company account to suspend, no platform to pressure, and no single computer to unplug.",
  },
  {
    title: "You hold the keys",
    body: "Your identity is a cryptographic keypair on your own machine, protected by a passphrase only you know. Every post you publish is signed by it, so readers can always verify it's really from you.",
  },
  {
    title: "Direct, non-custodial payments",
    body: "Subscriptions are paid wallet-to-wallet over the Lightning Network (via Nostr Wallet Connect). Money goes straight from a reader to a publisher, and no platform ever touches it.",
  },
  {
    title: "Free and open source",
    body: "Aetheria isn't a company or a product with a business model built on your data. The code is public, the protocol is documented, and anyone can run it.",
  },
];

export default async function Home() {
  const released = await isReleased();
  return (
    <div>
      <section className="max-w-5xl mx-auto px-6 pt-20 pb-16 text-center">
        <h1 className="text-4xl sm:text-5xl font-bold tracking-tight text-neutral-50">
          Publishing that can't be
          <br />
          <span className="aetheria-gradient-text">switched off</span>
        </h1>
        <p className="mt-6 text-lg text-neutral-400 max-w-2xl mx-auto leading-relaxed">
          Aetheria is a decentralized, serverless replacement for Substack or
          Medium. Your posts live on the Freenet peer-to-peer network instead
          of one company's servers. Nobody can deplatform you, and nobody
          stands between you and your readers.
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
