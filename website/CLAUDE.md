# Aetheria marketing/docs website

Separate Next.js (App Router, TypeScript, Tailwind v4) app, deployed to
Vercel independently from the desktop app in `../app/`. See the repo root
`CLAUDE.md` for the actual product; this file only covers this subproject.

Scaffolded with `create-next-app` on Next.js 16 / React 19 - APIs may
genuinely differ from older training data on Next.js conventions. If
something behaves unexpectedly, check `node_modules/next/dist/docs/` before
assuming a bug.

## Pages

- `/` - landing page.
- `/download` - download page. Two real files served as static assets from
  `public/downloads/`: `Aetheria-Setup-x64.exe` (full NSIS installer,
  bundles Freenet) and `Aetheria-app-only-x64.zip` (just `aetheria.exe` +
  `aetheria-delegate.exe`, for people already running their own Freenet
  node). Deliberately **not** GitHub Releases - the user wants the files
  served directly from this site.
- `/docs`, `/docs/getting-started`, `/docs/faq`, `/docs/security` - plain-
  language documentation for non-technical users, translating concepts
  already documented in the root `CLAUDE.md` (passphrase/no-recovery,
  Home-vs-Latest, the subscribe-to-others limitation, etc.) into copy a
  reader with no crypto/P2P background can follow.
- `/latest` - read-only view of real posts from the real network. No login,
  no posting, no delegate connection from the browser at all - see below
  for how the data actually gets here.

## The `/latest` page's data (why it's not truly live)

Vercel's serverless functions can't run a persistent Freenet node (a fresh
node needs real time to establish P2P ring connections - see the root
CLAUDE.md's environment notes), so `/latest` can't connect to Freenet
on-demand per visitor. Instead:

- `delegate/src/bin/snapshot_latest_feed.rs` (new `[[bin]]` target in the
  `aetheria-delegate` crate, reusing `contracts::fetch_global_directory` -
  read-only, no keys, no writes) connects to a real Freenet node and dumps
  the shared `GlobalDirectoryContract`'s current entries as JSON.
- **This accumulates rather than mirrors** (as of 2026-08-07): the tool
  takes the previous `latest-feed.json` as an optional arg and merges its
  entries with whatever the network returns this run, keyed by
  `post_contract_id`, capped at 100 (oldest evicted first) - so a post stays
  visible on the website even after it's fallen out of the live
  `GlobalDirectoryContract` view (that contract has its own, separate
  1000-entry cap and evicts its own oldest on overflow; Freenet can also
  prune a contract's state independently of that). See that file's module
  docs for the exact merge rule.
- That JSON is committed to `public/data/latest-feed.json` and the `/latest`
  page (`app/latest/page.tsx`) reads it as a plain file at request/build
  time via `fs.readFileSync` - no client-side fetch, no API route.
- `.github/workflows/refresh-latest-feed.yml` runs this on a schedule
  (`cargo install freenet`, run it briefly, run the snapshot tool, commit
  the JSON if it changed) so the file - and therefore the deployed page,
  once Vercel picks up the new commit - refreshes automatically without a
  separate always-on server. This trades genuinely-live data for zero extra
  hosting cost/ops, a deliberate tradeoff discussed with the user (see
  root CLAUDE.md's changelog around 2026-08-03) - a real always-on backend
  is the upgrade path if that tradeoff ever needs revisiting.
- To refresh it by hand: from `delegate/`, run
  `cargo run --release --bin snapshot-latest-feed ../website/public/data/latest-feed.json > /tmp/latest-feed.json && mv /tmp/latest-feed.json ../website/public/data/latest-feed.json`
  against a real reachable Freenet node, then commit the file. The previous-
  snapshot path must differ from wherever stdout is redirected to - a plain
  `>` truncates its target before the process gets a chance to read it back
  (this is why the CI workflow also writes to a `.new` file first, see its
  "Run snapshot" step).

## Release gate

`IsReleased` env var (`lib/config.ts`'s `isReleased()`, `=== "true"` check -
any other value, including unset, is treated as `false` and fails closed)
gates every download button on the site (`app/page.tsx`'s hero CTA,
`app/download/page.tsx`'s two file buttons). While `false`, those render a
"Coming soon!" message instead of a link - the actual files under
`public/downloads/` still exist and are technically reachable by direct
URL, this is a UI-level gate, not real access control. Flip it in Vercel's
project settings (Environment Variables) when actually ready to launch; see
`.env.example` for local testing (copy to `.env.local`, gitignored).

### Private preview bypass

`isReleased()` also returns `true` if the visitor holds a cookie named
`aetheria_preview` (see `PREVIEW_COOKIE_NAME` in `lib/config.ts`) - this
lets Hunter share the real download links with specific people before
flipping `IsReleased` on for everyone.

- Visiting `https://aetherianode.com/<PREVIEW_ACCESS_KEY>` sets that cookie
  (`app/[key]/route.ts` - a dynamic single-segment route that only matches
  when no static route already claims the path, so it never shadows
  `/download`, `/docs`, etc.) and redirects to `/`. Wrong or missing key just
  redirects home with no cookie set - no error page distinguishing valid
  from invalid keys.
- The key itself lives in the `PREVIEW_ACCESS_KEY` env var (Vercel project
  settings, or `.env.local` for local testing - see `.env.example`), never
  committed. Compared with `crypto.timingSafeEqual` to avoid a timing
  side-channel.
- Because `isReleased()` now reads a per-request cookie via `next/headers`,
  `/` and `/download` are dynamically rendered (can't be statically
  pre-rendered at build time) - expected and fine for a low-traffic
  marketing site.
- To rotate the key: generate a new random value, update
  `PREVIEW_ACCESS_KEY` in Vercel, send out the new link. The old key stops
  working immediately since there's nothing stateful to invalidate.

## Deploying

The user connects this repo's GitHub remote to a Vercel project themselves
and points a domain (registered/managed via Wix) at it - not something done
from here. The one thing Vercel needs told explicitly: set the project's
**Root Directory** to `website/` (this is a monorepo - `app/`, `delegate/`,
`contracts/`, and `website/` all live in the same repo, and Vercel doesn't
build from the repo root by default when the actual app lives in a
subdirectory).
