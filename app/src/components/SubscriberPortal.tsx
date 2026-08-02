export default function SubscriberPortal() {
  return (
    <div className="p-6">
      <h2 className="text-xl font-semibold mb-2">Subscribers</h2>
      <p className="text-sm text-neutral-500">
        Subscriber portal placeholder — will show tier pricing (from
        `PublisherProfileContract`) and a "Connect Wallet" NWC action that
        kicks off the invoice + ECDH key delivery flow (Workflow B).
      </p>
    </div>
  );
}
