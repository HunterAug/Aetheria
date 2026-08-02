export default function SubscriberPortal() {
  return (
    <div className="px-6 py-5">
      <h2 className="text-xl font-semibold text-neutral-100 mb-2">
        Subscribers
      </h2>
      <p className="text-sm text-neutral-500 max-w-md">
        Placeholder — will show tier pricing from{" "}
        <code className="text-aecyan-400">PublisherProfileContract</code> and
        a "Connect Wallet" NWC action that kicks off the invoice + ECDH key
        delivery flow (Workflow B).
      </p>
    </div>
  );
}
