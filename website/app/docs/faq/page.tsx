import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "FAQ | Aetheria",
  description: "Frequently asked questions about Aetheria.",
};

const QA = [
  {
    q: "How is this different from Substack or Medium?",
    a: "Those platforms work fine, until they don't: a Terms of Service update, an algorithm change, or an account suspension over a policy dispute (nothing illegal required) can cut you off from your archive and your readers with no appeal. Aetheria has no company running the platform at all, so there's no one positioned to do any of that. Once a reader has one of your posts, it's saved on their own device for good; no platform decision can reach back and take it away. Being hard to censor is part of that (there's no one to serve a takedown to), but it matters just as much for an ordinary newsletter writer who just doesn't want their work and their audience held hostage to a company's roadmap.",
  },
  {
    q: "What actually makes this “decentralized”?",
    a: "Your posts don't live on a server owned by a company called Aetheria. There isn't one. They're stored on Freenet, a peer-to-peer network where many independent computers each hold pieces of the network's data. There's no central database to seize, no company account to suspend, and no single point of failure.",
  },
  {
    q: "What happens if I turn my computer off?",
    a: "Once a post reaches the Freenet network, it's held by other peers too, not just your machine. That's the point. Your own computer doesn't need to stay online for your existing posts to remain readable. You do need your app running (and online) to publish new posts, read subscriber-only content, or manage your subscriptions.",
  },
  {
    q: "Is my writing permanent forever?",
    a: "Freenet keeps data that's actually being requested; content nobody looks at for a long time can fade from the network over time (the project's design plans a “pinning” feature to combat this, not yet built). In practice, anything with real readers stays alive. Separately, anything you've personally opened is saved on your own device the moment you read it. That copy is yours, readable offline, regardless of what later happens on the network.",
  },
  {
    q: "Who can read my subscriber-only posts?",
    a: "Only people who've paid and been granted the decryption key for that billing period. Everyone else, including someone browsing the Latest feed, sees just the title and a short summary, never the encrypted content.",
  },
  {
    q: "Do you take a cut of subscriptions?",
    a: "By default, no. 100% of a subscription payment goes directly from a reader's wallet to a publisher's wallet. The protocol optionally supports a small (2%) platform fee split if a specific build of the app is configured to collect one, but that's opt-in per deployment, not something baked in by default.",
  },
  {
    q: "Is this free?",
    a: "Yes. Aetheria is free, open-source software. There's no subscription to use the app itself, only the optional payments between publishers and their own subscribers.",
  },
  {
    q: "What is Freenet, exactly?",
    a: "Freenet is an independent, long-running peer-to-peer network project for censorship-resistant data storage, separate from Aetheria. Aetheria is one application built on top of it, the way a website is built on top of the internet.",
  },
  {
    q: "Can I use this on Mac or Linux?",
    a: "Not yet. The current build is Windows-only. The source is open if you want to build it for another platform yourself.",
  },
];

export default function Faq() {
  return (
    <div>
      <h1 className="text-3xl font-bold text-neutral-50">
        Frequently asked questions
      </h1>
      <div className="mt-10 space-y-8">
        {QA.map((item) => (
          <div key={item.q}>
            <h2 className="font-semibold text-neutral-100">{item.q}</h2>
            <p className="text-neutral-400 leading-relaxed mt-2">{item.a}</p>
          </div>
        ))}
      </div>
    </div>
  );
}
