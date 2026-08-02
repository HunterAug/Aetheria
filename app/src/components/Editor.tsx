export default function Editor() {
  return (
    <div className="p-6">
      <h2 className="text-xl font-semibold mb-2">Draft</h2>
      <p className="text-sm text-neutral-500 mb-4">
        Markdown editor placeholder — Phase 4 wires this into the Delegate's
        publish pipeline (draft → AES-GCM encrypt → sign → broadcast).
      </p>
      <textarea
        className="w-full h-96 rounded border border-neutral-300 p-3 font-mono text-sm"
        placeholder="Write your article in Markdown..."
      />
    </div>
  );
}
