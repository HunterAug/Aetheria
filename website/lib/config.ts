import { cookies } from "next/headers";

// The "flip the switch to go live" control for the whole site. Toggle
// `IsReleased` in Vercel's project settings (Environment Variables) for
// production, or in a local .env.local (see .env.example) for local testing.
//
// Defaults to `false` (not released) if the env var is missing entirely -
// fails closed, so forgetting to set it in a new environment hides the
// download buttons instead of accidentally shipping them early.
//
// A visitor holding the preview cookie (set by visiting
// /<PREVIEW_ACCESS_KEY>, see app/[key]/route.ts) also bypasses the gate -
// lets Hunter send a private early-access link before IsReleased flips for
// everyone.
export const PREVIEW_COOKIE_NAME = "aetheria_preview";

export async function isReleased(): Promise<boolean> {
  if (process.env.IsReleased === "true") return true;
  const store = await cookies();
  return store.get(PREVIEW_COOKIE_NAME)?.value === "1";
}
