import Link from "next/link";
import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Docs | Aetheria",
  description: "Documentation for Aetheria.",
};

const TOPICS = [
  {
    href: "/docs/getting-started",
    title: "Getting started",
    body: "Install Aetheria, create your identity, write your first post, and follow other publishers.",
  },
  {
    href: "/docs/faq",
    title: "FAQ",
    body: "What decentralized actually means here, what happens if a publisher's computer is off, and other common questions.",
  },
  {
    href: "/docs/security",
    title: "Security & your passphrase",
    body: "How your identity is protected, why there's no password reset, and what that means for you.",
  },
];

export default function DocsIndex() {
  return (
    <div>
      <h1 className="text-3xl font-bold text-neutral-50">Documentation</h1>
      <p className="mt-3 text-neutral-400">
        Plain-language guides for using Aetheria. No blockchain jargon
        required.
      </p>

      <div className="mt-10 space-y-4">
        {TOPICS.map((t) => (
          <Link
            key={t.href}
            href={t.href}
            className="block rounded-xl border border-ink-700 bg-ink-900 p-5 hover:border-ink-600 transition-colors"
          >
            <h2 className="font-semibold text-neutral-100">{t.title}</h2>
            <p className="text-sm text-neutral-400 mt-1">{t.body}</p>
          </Link>
        ))}
      </div>
    </div>
  );
}
