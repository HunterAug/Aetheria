import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Security & your passphrase — Aetheria",
  description: "How Aetheria protects your identity, and why there's no password reset.",
};

export default function Security() {
  return (
    <article>
      <h1 className="text-3xl font-bold text-neutral-50">
        Security &amp; your passphrase
      </h1>

      <h2 className="text-xl font-semibold text-neutral-100 mt-10 mb-3">
        What your passphrase actually protects
      </h2>
      <p className="text-neutral-400 leading-relaxed">
        When you create an identity, Aetheria generates a real cryptographic
        keypair and encrypts it on disk using a key derived from your
        passphrase. That keypair is what signs every post you publish (so
        readers can verify it&apos;s genuinely from you) and what a
        subscriber&apos;s payment gets tied to. Nobody — not even someone
        with access to your computer&apos;s files — can use that identity
        without your passphrase.
      </p>

      <h2 className="text-xl font-semibold text-neutral-100 mt-10 mb-3">
        Why there&apos;s no &quot;forgot password&quot;
      </h2>
      <p className="text-neutral-400 leading-relaxed">
        A password reset requires someone — a company, a server — to hold a
        way to recover or override your credentials. Aetheria doesn&apos;t
        have that, on purpose: nobody except you holds anything that could
        unlock your identity, which is exactly what makes it yours. The
        tradeoff is real and unavoidable:{" "}
        <strong className="text-neutral-300">
          if you lose your passphrase, that identity can&apos;t be recovered
          by anyone, including us.
        </strong>
      </p>
      <p className="text-neutral-400 leading-relaxed mt-3">
        Treat it like you would a crypto wallet seed phrase or a safe
        combination — write it down, store it somewhere durable, and
        don&apos;t rely on memory alone.
      </p>

      <h2 className="text-xl font-semibold text-neutral-100 mt-10 mb-3">
        What stays on your machine
      </h2>
      <p className="text-neutral-400 leading-relaxed">
        Your encrypted identity file and a local cache of your own posts
        live only on your computer. The Aetheria app talks to a local
        background process (the &quot;delegate&quot;) over a connection that
        never leaves your machine — your keys and any decrypted subscriber
        content never get sent anywhere except the signed, already-public
        data your posts are made of.
      </p>

      <h2 className="text-xl font-semibold text-neutral-100 mt-10 mb-3">
        What&apos;s public
      </h2>
      <p className="text-neutral-400 leading-relaxed">
        Once you publish, your public key, your public posts, and the
        title/summary of your subscriber-only posts are visible to anyone on
        the network — that&apos;s how following, discovery, and the Latest
        feed work. Only the full content of subscriber-only posts is
        encrypted.
      </p>
    </article>
  );
}
