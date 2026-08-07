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
- `/download` - download page, publicly live (no gate - see "Release gate
  removed" below). Real files served as static assets from
  `public/downloads/`, one section per platform: Windows
  (`Aetheria-Setup-x64.exe` full NSIS installer bundling Freenet, and
  `Aetheria-app-only-x64.zip` for people already running their own node),
  macOS (`Aetheria-Setup-macos-arm64.dmg`, Apple Silicon only), and Linux
  (`Aetheria-x86_64.AppImage`, `Aetheria-amd64.deb`, `Aetheria-x86_64.rpm`
  - see root CLAUDE.md's "Why Aetheria isn't a pure Freenet web-container
  app, and cross-platform builds" for how these get built). Deliberately
  **not** GitHub Releases - the user wants the files served directly from
  this site.
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

## Release gate removed (as of 2026-08-07)

The site used to gate every download button behind an `IsReleased` env var
(`lib/config.ts`'s `isReleased()`) plus a private-preview-cookie bypass
(`app/[key]/route.ts`, visiting `/<PREVIEW_ACCESS_KEY>`) for sharing real
download links before flipping that var on for everyone - see git history
around 2026-08-03 for how that worked. Removed outright, not just flipped
to `true`: the user was ready for a real public launch, at which point a
soft-launch/preview-only gate is dead weight rather than a feature to keep
around. `lib/config.ts` and `app/[key]/route.ts` are deleted; `/` and
`/download` now render their real content unconditionally (and are
statically prerenderable again, since neither reads request cookies
anymore). `.env.example` is gone too - there's no env var left for a fresh
clone to configure.

## Deploying

The user connects this repo's GitHub remote to a Vercel project themselves
and points a domain (registered/managed via Wix) at it - not something done
from here. The one thing Vercel needs told explicitly: set the project's
**Root Directory** to `website/` (this is a monorepo - `app/`, `delegate/`,
`contracts/`, and `website/` all live in the same repo, and Vercel doesn't
build from the repo root by default when the actual app lives in a
subdirectory).
