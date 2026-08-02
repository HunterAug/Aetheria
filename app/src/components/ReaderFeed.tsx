export default function ReaderFeed() {
  return (
    <div className="p-6">
      <h2 className="text-xl font-semibold mb-2">Feed</h2>
      <p className="text-sm text-neutral-500">
        Reader feed placeholder — will list `ContentIndexContract` entries
        fetched via the Delegate, with locked/unlocked state per post based
        on the subscriber's recovered epoch keys.
      </p>
    </div>
  );
}
