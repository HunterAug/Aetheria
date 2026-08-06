import Link from "next/link";
import { isReleased } from "@/lib/config";

const FEATURES = [
  {
    title: "Discover writers you actually care about",
    body: "Browse posts from everyone on the network or follow specific writers to curate your own feed. No algorithm decides what you see - you do. Follow anyone, unfollow instantly, and your feed updates in real-time.",
  },
  {
    title: "Once they have it, it's theirs to keep",
    body: "Posts are delivered directly to your readers over Freenet and saved on their devices. No algorithm can change what they see, no shutdown can delete their library, and no policy change can undeliver what's already in their hands.",
  },
  {
    title: "You hold the keys - literally",
    body: "Your identity is an Ed25519 keypair on your machine, encrypted with a passphrase only you know. Every post is cryptographically signed, so readers can always verify it's really from you. No account to hack, no company holding your identity.",
  },
  {
    title: "Direct payments, no middleman",
    body: "Offer paid tiers. Readers subscribe directly with their Lightning wallet. Money goes straight to you - Aetheria never touches it, takes a cut, or sees who paid whom.",
  },
];

export default async function Home() {
  const released = await isReleased();
  return (
    <div>
      <section className="max-w-5xl mx-auto px-6 pt-20 pb-16 text-center">
        <h1 className="text-4xl sm:text-5xl font-bold tracking-tight text-neutral-50">
          A social network you actually own.
          <br />
          <span className="aetheria-gradient-text">No company needed.</span>
        </h1>
        <p className="mt-6 text-lg text-neutral-400 max-w-2xl mx-auto leading-relaxed">
          Read what others are writing. Follow the voices you trust. Build an audience that's truly yours. Aetheria is a decentralized social network that works without servers, algorithms, or platforms - just peer-to-peer connections between writers and readers. Your posts live on your readers' devices forever. Your audience is yours to keep. And no company can ever shut you down, change the rules, or take it away.
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
          Discover what's being written right now
        </h2>
        <p className="mt-3 text-neutral-400 max-w-xl mx-auto">
          Browse the latest posts from across the Aetheria network. No installation needed - see the real community in action before you join.
        </p>
        <div className="mt-6">
          <Link
            href="/latest"
            className="inline-block rounded-lg border border-ink-700 text-sm font-semibold px-6 py-3 text-neutral-300 hover:bg-ink-900 transition"
          >
            Explore the network →
          </Link>
        </div>
      </section>
    </div>
  );
}
