import Link from "next/link";
import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Getting started | Aetheria",
  description: "Install Aetheria and publish your first post.",
};

export default function GettingStarted() {
  return (
    <article className="prose-like">
      <h1 className="text-3xl font-bold text-neutral-50">Getting started</h1>

      <h2 className="text-xl font-semibold text-neutral-100 mt-10 mb-3">
        1. Install
      </h2>
      <p className="text-neutral-400 leading-relaxed">
        Grab the installer from the{" "}
        <Link href="/download" className="text-aeblue-400 hover:underline">
          download page
        </Link>{" "}
        and run it. Windows will likely warn you it doesn&apos;t recognize the
        publisher. That&apos;s expected for a small open-source app, not a
        sign anything is wrong. Click through it.
      </p>

      <h2 className="text-xl font-semibold text-neutral-100 mt-10 mb-3">
        2. Create your identity
      </h2>
      <p className="text-neutral-400 leading-relaxed">
        The first time you open Aetheria, it asks you to choose a
        passphrase. This encrypts a cryptographic keypair stored only on
        your computer. It&apos;s how you sign everything you publish, and
        how readers can trust a post really came from you.
      </p>
      <p className="text-neutral-400 leading-relaxed mt-3">
        There&apos;s no account, no email, no company that can reset this
        passphrase for you.{" "}
        <strong className="text-neutral-300">
          If you forget it, that identity is gone for good.
        </strong>{" "}
        Write it down somewhere safe. See{" "}
        <Link href="/docs/security" className="text-aeblue-400 hover:underline">
          Security &amp; your passphrase
        </Link>{" "}
        for why it works this way.
      </p>

      <h2 className="text-xl font-semibold text-neutral-100 mt-10 mb-3">
        3. Write your first post
      </h2>
      <p className="text-neutral-400 leading-relaxed">
        Open the <strong className="text-neutral-300">Draft</strong> tab and
        write in Markdown. Every post is public. Anyone on the network can
        read it, no account or subscription needed on their end.
      </p>
      <p className="text-neutral-400 leading-relaxed mt-3">
        Publishing sends your post to the Freenet network, where it&apos;s
        stored redundantly across many peers instead of one server.
      </p>

      <h2 className="text-xl font-semibold text-neutral-100 mt-10 mb-3">
        4. Home vs. Latest
      </h2>
      <p className="text-neutral-400 leading-relaxed">
        <strong className="text-neutral-300">Home</strong> shows posts from
        publishers you follow.{" "}
        <strong className="text-neutral-300">Latest</strong> shows recent
        posts from everyone on the network, whether you follow them or
        not. It&apos;s how you find new publishers to begin with.
      </p>

      <h2 className="text-xl font-semibold text-neutral-100 mt-10 mb-3">
        5. Follow someone
      </h2>
      <p className="text-neutral-400 leading-relaxed">
        Click any author&apos;s name (in Home, Latest, or on a post you
        opened) to see their profile and a Follow button. You can also
        paste a publisher&apos;s public key directly on the Following tab if
        you know it but haven&apos;t seen a post of theirs yet.
      </p>
    </article>
  );
}
