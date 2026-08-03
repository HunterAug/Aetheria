// The "flip the switch to go live" control for the whole site. Toggle it in
// Vercel's project settings (Environment Variables) for production, or in a
// local .env.local (see .env.example) for local testing.
//
// Defaults to `false` (not released) if the env var is missing entirely -
// fails closed, so forgetting to set it in a new environment hides the
// download buttons instead of accidentally shipping them early.
export const IS_RELEASED = process.env.IsReleased === "true";
