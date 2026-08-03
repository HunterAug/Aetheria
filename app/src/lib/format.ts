// Small display-formatting helpers shared by the feed/profile components -
// factored out of ReaderFeed.tsx once Following.tsx needed the same
// relative-time/initial-avatar/short-pubkey logic instead of duplicating it.

export function relativeTime(unixSeconds: number): string {
  const diffSec = Math.max(0, Date.now() / 1000 - unixSeconds);
  const units: [number, string][] = [
    [60, "s"],
    [60, "m"],
    [24, "h"],
    [365, "d"],
  ];
  let value = diffSec;
  let suffix = "s";
  for (const [size, label] of units) {
    if (value < size) {
      suffix = label;
      break;
    }
    value /= size;
    suffix = label;
  }
  return `${Math.max(1, Math.floor(value))}${suffix}`;
}

export function initial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "A";
}

export function shortHex(hex: string): string {
  return hex.length > 16 ? `${hex.slice(0, 8)}…${hex.slice(-6)}` : hex;
}
