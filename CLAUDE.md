# Aetheria — Project Memory

Decentralized, censorship-resistant Substack/Medium replacement on Freenet.
Full spec: `docs/Decentralized_Substack_Design_Doc.pdf` (read this before
making architectural changes — it's the source of truth for data schemas,
crypto flows, and the 16-week roadmap).

## Layout

- `contracts/` — Rust workspace, Freenet WASM state contracts (Layer 3).
  Compiles to `wasm32-unknown-unknown` via `freenet-stdlib`'s `ContractInterface`
  trait (`validate_state` / `update_state` / `summarize_state` / `get_state_delta`).
  - `aetheria-types/` — shared structs used across contracts and the delegate.
  - `publisher-profile-contract/`, `content-index-contract/`,
    `post-data-contract/` — one crate per contract from design doc §3.
    (`subscriber-registry-contract/` existed here too until payments/
    subscriptions were removed - see "Payments and subscriptions removed"
    below.)
  - `global-directory-contract/` — **not** in the design doc; backs the
    Latest (network-wide) feed. See "Home = following-only feed, Latest =
    network-wide feed" below for why it exists.
- `delegate/` — native Rust daemon (Tokio), Layer 2. Owns keys, the Freenet
  bridge, and a local SQLite cache. Never expose key material across the IPC
  boundary to the UI — only content and derived state. Both a library
  (`src/lib.rs`, `pub mod`s for
  `contracts`/`freenet_bridge`/etc.) and two binaries: `aetheria-delegate`
  (`src/main.rs`, the real daemon - thin wrapper around the library) and
  `snapshot-latest-feed` (`src/bin/`, read-only, feeds `website/`'s Latest
  page - see below).
- `app/` — React 18 + TypeScript + Tailwind + Tauri, Layer 1. Talks to the
  delegate only via the loopback WebSocket in `app/src/lib/delegate.ts`.
- `website/` — separate Next.js marketing/docs site (its own `CLAUDE.md`),
  deployed independently (Vercel) from the desktop app. Download page,
  plain-language docs, and a read-only `/latest` feed viewer. See "Marketing
  website" below.

## Dev scripts (as of 2026-08-03)

`scripts/` collapses the multi-step build/start/stop sequences this file's
own history shows getting run by hand, repeatedly, into single commands -
written after those manual sequences had already caused a real mistake once
(a stale installer, see the Freenet-sidecar section below) and were eating a
lot of back-and-forth. All three are plain bash, run from anywhere via
`bash scripts/<name>.sh` (git-bash, already on this machine).

- **`scripts/build.sh`** - `cargo build --release` (delegate) → copy into
  the Tauri sidecar dir → `npm run build:desktop` (frontend + Tauri shell +
  both installers) → **byte-compares** the fresh delegate build against both
  the sidecar copy and `builds/aetheria-delegate.exe`, and fails loudly if
  they differ. That comparison is deliberately just a file diff, not an IPC
  feature check (e.g. "does it recognize op X") - a feature check needs
  updating every time a new op is added, a byte comparison catches "shipped
  a stale binary" unconditionally forever. Exports `PATH` for `cargo` itself
  (not always present in a fresh shell on this machine, see below) so it
  doesn't depend on the caller remembering to.
- **`scripts/dev-up.sh`** - confirms the real Freenet dev service is
  reachable (starts it if not - `freenet.exe service start`), then starts
  the delegate release binary against the real local identity
  (`AETHERIA_DEV_PASSPHRASE=aetheria-dev-local-only`, this machine's
  documented dev convention). Idempotent - a second call detects the
  already-running delegate via its pidfile and no-ops. Writes
  `.dev-delegate.pid` (gitignored) and `delegate.log` (gitignored, also
  where startup errors go rather than the terminal). Deliberately does
  **not** start the Vite dev server - use the Browser tool's `preview_start`
  (`"aetheria-frontend"`, see `.claude/launch.json`) for that; it already
  runs `npm run dev` with proper log/tab integration, this would just be a
  worse duplicate.
- **`scripts/dev-down.sh`** - stops the delegate `dev-up.sh` started (via
  its pidfile, falling back to killing by process name if the pidfile's
  missing - e.g. a delegate started outside this script). Leaves the
  persistent Freenet dev service running by default, since it's a standing
  part of the environment that predates any given session, not something to
  tear down casually - pass `--with-freenet` to also stop it.
- **Real gotcha found writing these, worth knowing generally**: git-bash's
  `$!` after backgrounding a Windows `.exe` with `&` is an MSYS-internal
  pseudo-PID, **not** the real Windows PID `Get-Process`/`Stop-Process`
  need (confirmed via `ps -W`'s WINPID column showing a different number
  entirely). `dev-up.sh` looks the real PID up by process name afterward
  instead of trusting `$!` - trusting it silently produces a pidfile that
  can't actually stop anything.

## Environment notes (this machine)

- Rust toolchain (`rustup`/`cargo`) and the `wasm32-unknown-unknown` target
  are installed. `cargo check` and `cargo build --target wasm32-unknown-unknown`
  both pass warning-free in `contracts/` as of the commits below. `cargo` may
  not be on `PATH` in every shell — if `command not found`, add
  `C:\Users\WebDev\.cargo\bin` to `PATH` for that session.
- **No GitHub CLI (`gh`) installed.** Repo is pushed to GitHub by adding a
  remote manually (`git remote add origin <url>`) rather than `gh repo create`.
- **Port 3000 is occupied by something else on this machine** — the Vite
  dev server uses **5173** instead, and the delegate's IPC WebSocket uses
  **47021**. Don't move either back onto 3000.
- A real `freenet` node is installed and runs in normal (network) mode,
  connected to the public gateway network — dashboard at
  `http://127.0.0.1:7509/`. Its **client WebSocket API is
  `ws://127.0.0.1:7509/v1/contract/command?encodingProtocol=native`** - NOT
  the root path `/` (that serves the HTML dashboard for any request,
  upgrade headers included, and fails a `tokio-tungstenite` handshake with
  "HTTP error: 200 OK" instead of `101 Switching Protocols`), and NOT the
  path alone without `?encodingProtocol=native` (you get a `101`, but the
  first real response fails `bincode::deserialize` with "invalid value:
  integer `12`, expected `Ok` or `Err`" - the node defaults to a different,
  presumably flatbuffers, wire encoding otherwise). Found by reading `fdev`'s
  own connect call (`fdev-0.3.280/src/commands/v1.rs`, cached locally in the
  cargo registry) rather than guessing from `freenet-stdlib`'s protocol
  docs. The node currently shows "only connected to gateways, NAT
  hole-punching 0/N" — that's about *other peers reaching this node*, not
  about this node's own ability to GET/PUT contract state through its
  gateways.
- **The real network is flaky in ways worth retrying, not debugging.**
  PUT/UPDATE/GET against the public gateway network intermittently fail
  with e.g. "put timed out after 1 peer attempt(s) (0 infra-retries on same
  peer)" or "awaited peer \<addr\> disconnected before replying" - freenet-core
  does not retry these itself. `delegate/src/freenet_bridge.rs` retries each
  operation up to 4 times with a 1.5s delay client-side; even that isn't
  always enough; the same request from a fresh process sometimes succeeds
  immediately. This is inherent to the current network, not a bug in the
  delegate - don't try to "fix" it away, just expect it when testing.
- `fdev` (Freenet's dev CLI, crate `fdev` on crates.io — `cargo install fdev`,
  currently 0.3.280) is installed. **Known bug**: `fdev build` panics
  "Could not find workspace root" when installed via `cargo install` from
  crates.io, because `get_workspace_target_dir()` in its `util.rs` uses
  `env!("CARGO_MANIFEST_DIR")` - baked in at *fdev's own* compile time
  (pointing into the cargo registry cache) - instead of the caller's actual
  working directory, then searches that path's ancestors for a `[workspace]`
  Cargo.toml and finds none. Workaround: set `CARGO_TARGET_DIR` yourself
  before invoking (it short-circuits the broken lookup), e.g.
  `CARGO_TARGET_DIR=./contracts/target fdev build` from inside a contract
  crate dir. Also: every contract needs a `freenet.toml` next to its
  `Cargo.toml` (`[contract]\ntype = "standard"\nlang = "rust"`) or `fdev
  build`/`publish` refuse to run - all four contract crates have one.
- **Verified 2026-08-02**: built `post-data-contract` with `fdev build`,
  published a real `EncryptedPostPayload` state to the running local node
  with `fdev -p 7509 publish --code <path> --subscribe contract --state
  <cbor-file>`, and independently read it back with `fdev -p 7509 execute
  get <contract-key>` - bytes matched exactly when decoded with our own
  `aetheria-types::EncryptedPostPayload`. This is real Freenet network
  integration (not the local-SQLite-only milestone below). Note flag order
  matters: `--subscribe` goes *before* the `contract` subcommand, not after.
  Also: `fdev publish ... --subscribe` prints an "Error: Unexpected contract
  response: UpdateNotification { ... }" - that's `fdev`'s CLI failing to
  parse a subscription push notification, not a real failure; the state
  inside that error *is* your published data being echoed back.
- `.claude/launch.json` defines two preview_start configs: `aetheria-frontend`
  (`npm run dev --prefix app` on port 5173) and `freenet-node` (attaches to
  the already-running node's dashboard at `http://127.0.0.1:7509`, no
  process started). Browser-tool navigation to arbitrary localhost URLs is
  otherwise blocked; use `preview_start` with a registered config instead.
- The delegate's local data (SQLite cache + identity key) lives in the
  platform app-data dir via `directories::ProjectDirs` — on this machine
  that's `%APPDATA%\aetheria\aetheria-delegate\data\`. It is **not** relative
  to wherever the binary happens to be launched from (see the git history
  around 2026-08-02 for why that distinction mattered — a CWD-relative path
  silently forked the cache across multiple stray folders).
- **`%APPDATA%\aetheria\aetheria-delegate\data.stale-wrong-passphrase\`
  exists on this machine** (found 2026-08-02 while verifying the Freenet
  sidecar work) - an old `identity.key` that does **not** decrypt under the
  documented `AETHERIA_DEV_PASSPHRASE`, alongside a much smaller/staler
  SQLite cache than the real `data\` directory. Deliberately preserved
  rather than deleted (unclear provenance - possibly an earlier passphrase
  actually used before the `aetheria-dev-local-only` convention was
  settled on), not integrated into anything. Safe to delete if it's not
  needed, just hasn't been confirmed disposable.

## Conventions

- Contract state structs live in `aetheria-types` if used by more than one
  contract or by the delegate; contract-local structs stay in that crate.
- Unimplemented subsystems are marked with `todo!()` plus a `// TODO(PhaseN):`
  comment citing the doc section, not silently stubbed with fake success.
- Every contract crate under `contracts/*-contract/` needs **both**:
  `freenet-stdlib = { version = "0.8", features = ["contract"] }` (unlocks
  `freenet_stdlib::memory::wasm_interface`, used by the `#[contract]` macro
  expansion) **and** its own `[features] default = ["freenet-main-contract"]`
  / `freenet-main-contract = []` (the macro's `#[no_mangle] extern "C"` WASM
  exports are gated on this feature name, but the `cfg` check resolves
  against the crate the macro expands into — not `freenet-stdlib` — so it
  has to be declared here too). Without both, `cargo check` "succeeds" but
  silently drops the WASM exports, and every helper the trait impl calls
  looks like unused dead code. Copy this pattern for any new contract crate.

## Working end-to-end (as of 2026-08-02)

The publisher's own publish → encrypt → feed → decrypt loop works for real,
verified live in the browser, entirely local (no Freenet, no NWC):

- `delegate/src/ipc.rs` implements a JSON request/response protocol over the
  loopback WebSocket: `list_posts`, `get_post`, `publish_post`. See that
  file's `Request` enum for the exact shape.
- `app/src/lib/delegate.ts` is the typed client; `Editor.tsx` and
  `ReaderFeed.tsx` use it instead of rendering placeholders.
- Public posts are stored as plaintext in SQLite; subscriber-only posts are
  AES-256-GCM encrypted under a per-epoch key (`current_epoch_id()` in
  `ipc.rs` currently buckets by ~30-day windows, not real calendar months —
  that's a known simplification, see TODO there).
- Since there's only ever one identity (the publisher = the reader in this
  milestone), decryption always succeeds locally — this does **not** yet
  test the actual ECDH subscriber key-delivery path.

**Real Freenet network integration is now wired up and verified live**
(`delegate/src/freenet_bridge.rs`, `delegate/src/contracts.rs`), on top of
the local loop above, which keeps working unchanged:

- `FreenetBridge` uses `freenet_stdlib::client_api::WebApi` directly (no
  shelling out to `fdev`) for real `Put`/`Update`/`Get` against
  `ws://127.0.0.1:7509/v1/contract/command?encodingProtocol=native` (see the
  environment note above for why that exact URL, not just the host:port).
- Compiled contract WASM (`PublisherProfileContract`, `ContentIndexContract`,
  `PostDataContract`) is embedded into the delegate binary via
  `include_bytes!` pointing at each contract's `fdev build` output under its
  (gitignored) `build/freenet/` directory - **you must run `fdev build`
  for these three contracts before building the delegate** if their source
  changed (see the `fdev build` / `CARGO_TARGET_DIR` note above; same
  invocation, once per contract crate). This was the simplest working option
  of the three considered (shell out to `fdev` at runtime; embed via
  `include_bytes!` with a manual/CI build step; PUT code without `fdev` at
  all) - it needed no new runtime dependency and `ContractRequest::Put`
  already carries the full `ContractContainer` (code + params), so the node
  never needs the code pre-cached.
- On first run, the delegate publishes a fresh, empty `ContentIndexContract`
  and a signed `PublisherProfileContract` pointing at it; both keys persist
  in the SQLite `contract_registry` table (`db::LocalStore::{get,set}_contract_registration`)
  and are reused (no re-publish, no network call) on every later run -
  verified by killing and restarting the delegate process and confirming the
  logged keys were identical and no "published X" log lines appeared.
- `ipc.rs`'s `publish_post` now does both: the existing SQLite write (public
  plaintext / subscriber AES-256-GCM ciphertext, unchanged), *and* mints a
  fresh `PostDataContract` instance (`Parameters` = the post's 16-byte
  `post_id`, so one shared compiled contract yields one instance key per
  post) holding the same payload, then folds a signed `PostMetadataHeader`
  for it into the publisher's `ContentIndexContract` via a full-state
  `Update` (`ContentIndexContract::update_state` merges by `post_id`
  itself, so a full-state resend is simpler than computing a delta
  client-side, and idempotent under the retries below).
- `ContentIndexState` and the wire shape of `PublisherProfile` are
  hand-mirrored in `delegate/src/contracts.rs` rather than imported from the
  contract crates (see that file's module docs for why - the `#[contract]`
  macro's WASM exports risk colliding if more than one contract crate is
  linked into one native binary). Keep both in sync by hand if either
  contract's state struct changes.
- **Verified live, 2026-08-02**, full round trip through the real IPC
  protocol (`publish_post` over `ws://127.0.0.1:47021`) against the real
  running node, independently confirmed with `fdev -p 7509 execute get
  <key>` for every contract key involved (separate process, same
  methodology as the earlier `fdev`-only verification):
  - `PublisherProfileContract` `Ef8afAdP2UsuNmXQ6mF35miawtDbPbQkVLUEaeESPL7U`
    - `content_index_contract_id` field correctly points at the
    `ContentIndexContract` key below.
  - `ContentIndexContract` `8QshRgPQtB9YBFxqLHRzMgHTiThUBZPkKPstbMrwmXLg` -
    grew from an empty `posts: []` to two entries as posts were published,
    each with a `post_contract_id` matching a real `PostDataContract` key.
  - Public post → `PostDataContract` `62fpzvdb6KZ9UU2ehbbgeDJ51GNgaQ52W562mHS9wiNE`
    - `cipher_text` holds the literal plaintext markdown, `nonce` all-zero,
    matching the contract's documented public-post convention.
  - Subscriber-only post → `PostDataContract`
    `Hdh6LXTR4Kkaq4URoJoRtmjdX74MhEumSWJvsd1D5KAC` - `cipher_text` is
    genuine AES-256-GCM ciphertext (unreadable garbage), `nonce` is
    non-zero-random, confirming real encryption reached the network.
  - Local SQLite `list_posts`/`get_post` kept working throughout (both
    posts decrypt/read back correctly from the local cache).

## Identity key encryption (as of 2026-08-02)

`delegate/src/keys.rs`'s `identity.key` is now encrypted at rest
(Argon2id-derived key wrapping the 64 bytes of key material with
AES-256-GCM) instead of plaintext. **This means `load_or_generate` now
blocks on an interactive passphrase prompt (`rpassword`) every time the
delegate starts** - it reads from the real console device on Windows, not
redirected/piped stdin (confirmed via `rpassword`'s own
`tests/no-terminal.rs`), so a delegate launched as a background/piped
process (as this session has been doing all along for testing) will hang
at that prompt with no way to answer it from here. **Whoever runs the
delegate needs a real interactive terminal to enter the passphrase into**
- this is a genuine workflow change from before, not a bug to route around.
Covered by unit tests in `keys.rs` that exercise the encryption/migration
logic directly (bypassing the prompt) instead.

The pre-existing plaintext `identity.key` on this machine corresponds to a
real pubkey with already-published `PublisherProfileContract`/
`ContentIndexContract` instances (see keys above) - migration re-encrypts
those *same* key bytes rather than generating a fresh keypair, so the
existing contracts stay signable. The user, not this session, needs to be
the one who runs that first migration and sets the passphrase, since they're
the one who has to remember it afterward.

This is now solved for real - see "Bundled Freenet node + real in-app
passphrase unlock" below for the `unlock { passphrase }` IPC message and the
locked/unlocked startup restructuring it needed.

**Dev convenience, approved by the user for local testing only**: set
`AETHERIA_DEV_PASSPHRASE` in the environment to skip the interactive prompt
entirely (used for both the migration and unlock paths) - loudly logged as
insecure every time it's used. This is what re-enabled unattended
start/stop of the delegate during this session's testing; the real
identity's plaintext file was migrated using this env var on 2026-08-02
(passphrase `aetheria-dev-local-only`, chosen for this dev machine only -
this is not a secret worth protecting, treat it as public). Never set this
outside a local dev loop.

## Desktop shell (Tauri) — real window + auto-started delegate (as of 2026-08-02)

`app/src-tauri/` now actually builds and runs. It was an early scaffold
(`Cargo.toml`, `tauri.conf.json`, a placeholder icon) that had never been
launched; getting `npm run tauri dev` (from `app/`) working for real
surfaced a few problems, fixed as follows:

- The scaffold's `Cargo.toml` had a `[lib] name = "aetheria_lib"` target
  (for a mobile-capable `create-tauri-app` template) with no corresponding
  `src/lib.rs` - `cargo metadata` failed outright (`can't find library
  aetheria_lib`). Removed the `[lib]` section entirely; this app is
  desktop-only (no iOS/Android targets planned), and all the logic already
  lived in `src/main.rs`.
- `tauri`/`tauri-build` bumped to the current 2.x line (2.11.x resolved from
  `version = "2"`) - the scaffold's versions were already coherent Tauri v2,
  just untested.
- Added `tauri-plugin-shell = "2"` (2.3.5 resolved) to run the delegate as a
  **sidecar** - Tauri's mechanism for bundling an external binary and
  spawning/killing it as a managed child process
  (https://v2.tauri.app/develop/sidecar/). `app/src-tauri/src/main.rs`'s
  `.setup()` hook calls `app.shell().sidecar("aetheria-delegate")...spawn()`
  on startup, and the `.run(|app_handle, event| ...)` closure kills the
  child on `RunEvent::ExitRequested`/`RunEvent::Exit` so the delegate never
  outlives the window (verified - see below). Since the frontend never
  calls `invoke()` for anything (it talks to the delegate directly over the
  `ws://127.0.0.1:47021` loopback socket, see `app/src/lib/delegate.ts`),
  spawning the sidecar from Rust needed no capability/permission grant for
  it; added a minimal `capabilities/default.json` (`core:default` only)
  since Tauri v2 expects at least one capabilities file to exist.
- **Sidecar binary naming**: Tauri resolves `externalBin` entries by
  appending `-<host-target-triple><exe-suffix>` to the given name. With
  `"externalBin": ["binaries/aetheria-delegate", "binaries/freenet"]` in
  `tauri.conf.json` and `.sidecar("aetheria-delegate")`/`.sidecar("freenet")`
  in Rust, the actual files must exist at
  `app/src-tauri/binaries/aetheria-delegate-x86_64-pc-windows-msvc.exe` and
  `app/src-tauri/binaries/freenet-x86_64-pc-windows-msvc.exe` on this
  machine. Neither is **source** - the delegate one is a straight copy of
  `delegate/target/{debug,release}/aetheria-delegate.exe`, the freenet one a
  straight copy of `C:\Users\WebDev\AppData\Local\Freenet\bin\freenet.exe`
  (gitignored, `app/src-tauri/binaries/` in the root `.gitignore`), and both
  must be regenerated locally before `npm run tauri dev`/`build` after any
  delegate rebuild (freenet's copy only needs redoing if you want to bundle a
  newer Freenet version):
  ```
  cp delegate/target/debug/aetheria-delegate.exe \
     app/src-tauri/binaries/aetheria-delegate-x86_64-pc-windows-msvc.exe
  cp "C:\Users\WebDev\AppData\Local\Freenet\bin\freenet.exe" \
     app/src-tauri/binaries/freenet-x86_64-pc-windows-msvc.exe
  ```
  (swap `debug` for `release` for a production bundle). See "Bundled Freenet
  node" below for why a second sidecar exists at all, and
  `THIRD_PARTY_LICENSES.md` for the AGPL-3.0 redistribution note.
- **Vite/Tauri interaction bug**: `tauri dev` builds the Rust shell inside
  `app/src-tauri/target/` while Vite's dev server is also running from
  `app/`. Vite's default file watcher picks up churn in that directory
  (including a build-script binary mid-write/locked by cargo), and on
  Windows that throws an `EBUSY` error that crashes Vite's process instead
  of just logging a warning - which in turn kills `tauri dev`'s
  `beforeDevCommand` step. Fixed in `app/vite.config.ts` with
  `server.watch.ignored: ["**/src-tauri/**"]`.
- **Passphrase unlock**: the sidecar is spawned with no passphrase env var
  at all - `delegate/src/keys.rs`'s encrypted-identity unlock no longer needs
  one at startup, because the delegate now starts locked and the real
  in-app `UnlockScreen` sends an `unlock { passphrase }` IPC request. See
  "Bundled Freenet node + real in-app passphrase unlock" below for the full
  story; this used to be a hardcoded `AETHERIA_DEV_PASSPHRASE` stopgap.
- **Icons**: regenerated the full multi-resolution set with
  `npx tauri icon ../logo.png` (run from `app/`, source is the real 1024x1024
  logo at repo root / `app/public/logo.png`) - replaced the old placeholder
  `icons/icon.png` with `icons/{32x32,64x64,128x128,128x128@2x,icon}.png`,
  `icon.icns`, `icon.ico`, plus Android/iOS/Windows-Store variants the CLI
  also generates unconditionally. `tauri.conf.json`'s `bundle.icon` now
  lists the specific PNG/ICO/ICNS files Tauri's Windows/macOS bundlers
  expect, not the old single placeholder path.

**Verified end-to-end, 2026-08-02** (`npm run tauri dev` from `app/`, real
window, not just the browser dev server):

- A real native OS window titled "Aetheria" opens (confirmed via
  `Get-Process aetheria | Select MainWindowTitle`, not just process
  existence).
- `aetheria-delegate.exe` auto-starts as a child of `aetheria.exe` with no
  manual launch (process start timestamps one second apart), binds
  `127.0.0.1:47021`, and the app's webview immediately opens a WebSocket to
  it (confirmed via `netstat` showing an ESTABLISHED pair, and via the
  delegate's own forwarded logs: identity unlocked non-interactively via the
  dev passphrase, then "Freenet publisher identity ready" with the *same*
  `content_index`/`publisher_profile` contract IDs already documented above
  - i.e. it's genuinely reusing the real persisted identity/contracts, not
    generating fresh ones).
- The feed actually renders real posts (checked by pointing the browser
  tool at the same `devUrl`, `http://localhost:5173`, which is the identical
  content the native webview loads - the real posts from the "Working
  end-to-end" section above all appeared).
- Closing the window gracefully (`WM_CLOSE`, not a force-kill) exits both
  `aetheria.exe` and `aetheria-delegate.exe` cleanly - confirms the
  `RunEvent::Exit` cleanup handler in `main.rs` actually kills the sidecar
  rather than leaking it. (A hard `TerminateProcess`/force-kill of
  `aetheria.exe`, tested separately, does *not* run this cleanup - that's
  expected of any process, not a bug, and not how a user closing the app
  window behaves.)

**`npm run tauri build`**: succeeded completely, producing both installers:
`app/src-tauri/target/release/bundle/msi/Aetheria_0.1.0_x64_en-US.msi`
(7.7MB) and `.../bundle/nsis/Aetheria_0.1.0_x64-setup.exe` (5.2MB) - Tauri
downloaded WiX3 and NSIS toolchains on the fly, no manual installer tooling
needed. No code-signing was configured (would need a cert), so both
installers will show an "unknown publisher" warning on install - fine to
leave unresolved per the task, noted here as a follow-up rather than a
blocker.

That debug-binary caveat is resolved: as of the Freenet-sidecar/unlock-screen
work below, `delegate/target/release/aetheria-delegate.exe` builds clean
(`cargo build --release`, no errors, no warnings) and is what's actually
copied into `app/src-tauri/binaries/` and bundled into installers now.

## Bundled Freenet node + real in-app passphrase unlock (as of 2026-08-02)

Two things needed for a real installer that works on someone else's PC with
zero manual setup: (1) the app was unusable without a Freenet node already
running, since nothing installed or started one; (2) every fresh install
would have generated a new identity encrypted under the same hardcoded,
publicly-known dev passphrase, providing no real protection.

### Part 1: Freenet bundled as a second Tauri sidecar

`app/src-tauri/src/main.rs`'s `.setup()` hook now spawns **two** sidecars,
`freenet` before `aetheria-delegate` (see "Sidecar binary naming" above):
`shell.sidecar("freenet").args(["network"]).spawn()` - plain `network` mode,
deliberately **not** `service`/`service run-wrapper` (Freenet's own
persistent-background-install layer, which does onboarding/browser-opening
and its own crash-loop-with-backoff supervision - redundant and actively
confusing once Tauri is already supervising this as a child process; verified
by running a real `freenet.exe network` instance against a totally empty
scratch `--data-dir`/`--config-dir` with stdin closed - it bound its
WebSocket API and produced zero "onboard"/"dashboard" log lines, vs. the
`service` wrapper's real historical log on this machine showing a
"First-run onboarding: dashboard opened" line and a 45-minute crash-loop
sequence the first time this machine's Freenet install went through it).

- **License**: freenet-core is **AGPL-3.0** (`freenet-stdlib`, which
  Aetheria's own Rust code links against directly, is separately licensed
  LGPL by the same project specifically so that's safe for a proprietary
  app - only the `freenet-core` node binary itself is AGPL). Per
  freenet-core's own `LICENSE.md`: "applications merely communicating with
  Freenet over standard protocols (HTTP, WebSocket) without directly linking
  to freenet-core are not derivative works subject to AGPL requirements" -
  Aetheria's delegate only ever talks to the bundled node over its loopback
  WebSocket API, never links against it, so bundling the two binaries
  together in one installer is mere aggregation, not a combined/derivative
  work requiring Aetheria itself to be AGPL. The bundled binary is conveyed
  unmodified, which AGPL permits with notices intact. Full writeup,
  including the exact bundled version/commit, in `THIRD_PARTY_LICENSES.md`
  at the repo root - not legal advice, but a real citation of the project's
  own licensing summary rather than a guess.
- **Startup sequencing**: rather than a fixed sleep in `main.rs` between
  spawning the two sidecars, `FreenetBridge::connect_local()` itself
  (`delegate/src/freenet_bridge.rs`) now retries the initial connection with
  a fixed 1.5s delay for up to 20 attempts (30s total) before giving up -
  distinct from `MAX_ATTEMPTS`/`RETRY_DELAY` above it, which govern
  individual contract operations on an *already-established* connection
  (different failure mode: "nothing listening on 7509 yet" vs. "the gateway
  network is flaky"). This also helps the pre-existing CLI-launched delegate
  if someone starts it before Freenet has finished booting, not just the new
  sidecar case.
- **Data directory isolation**: Freenet's own data lives at
  `%LOCALAPPDATA%\The Freenet Project Inc\Freenet\data\` on Windows (found
  by inspecting the real running node's logs, e.g. `node_kek`'s path),
  completely separate from Aetheria's own
  `%APPDATA%\aetheria\aetheria-delegate\data\` - no override needed or
  applied for the bundled sidecar's normal path.
- **Dev/test escape hatches** (both off by default, never touch real data
  unless explicitly set): `AETHERIA_DATA_DIR_OVERRIDE` (read by
  `delegate/src/main.rs::local_data_dir`) redirects the delegate's own data
  dir; `AETHERIA_FREENET_DATA_DIR_OVERRIDE` (read by the Tauri shell's
  `main.rs`) passes `--data-dir`/`--config-dir` through to the bundled
  Freenet sidecar. Added specifically so a "fresh machine" test never has to
  touch this dev machine's real identity/contracts or real Freenet peer
  state - `directories::ProjectDirs` resolves via the Windows known-folder
  API, which ignores plain `%APPDATA%`/`%LOCALAPPDATA%` env var overrides,
  so redirecting it needs an explicit escape hatch like this rather than
  just setting those.
- **Exit cleanup**: `RunEvent::ExitRequested`/`Exit` now kills both sidecars
  (a `Vec<CommandChild>`, LIFO order - delegate first, then the node it was
  talking to), not just the delegate.
- **Verified for real, 2026-08-02**: built the full installer
  (`npm run build:desktop`, both MSI and NSIS), silently installed it
  (`Aetheria_0.1.0_x64-setup.exe /S`) to `%LOCALAPPDATA%\Aetheria\` (confirmed
  all three binaries present), then launched the installed `aetheria.exe`
  with both scratch-dir env vars pointed at brand-new empty directories (the
  real live Freenet service was cleanly `service stop`/`service start`-paused
  around this one test to free port 7509, its own data untouched throughout -
  confirmed identical before/after via the same content_index/publisher_profile
  keys this file already documents). Confirmed via `netstat`: both the fresh
  Freenet sidecar (port 7509) and the delegate (port 47021) bound with no
  collision. Drove a real IPC round trip (`get_profile` → `publish_post` →
  `get_post`) from a small Node script talking to `ws://127.0.0.1:47021` -
  `publish_post` returned `network_synced: true` with a real
  `PostDataContract` id, and `get_post` read back the exact markdown,
  confirming the full chain (fresh sidecar bind → delegate connect-with-retry
  → identity PUT → post PUT → GET) actually works, not just that processes
  started. The real native window also opened and correctly showed the
  first-run display-name prompt for the fresh identity. Closing the window
  gracefully killed both sidecars and freed both ports. Uninstalled cleanly
  afterward (`uninstall.exe /S`) and restarted the real Freenet service.

**Known distribution friction points** (real, not yet addressed - the
installer works, but a first-time recipient will likely hit one or more of
these):

- **Unsigned installer** - no code-signing cert is configured
  (`bundle.windows` has no signing config in `tauri.conf.json`), so Windows
  SmartScreen will very likely show "Windows protected your PC" on first
  run of the `.exe`/`.msi`. Needs a real code-signing certificate to fix.
- **x64 Windows only** - this build targets `x86_64-pc-windows-msvc`
  specifically; won't run on macOS, Linux, or ARM Windows.
- **Possible firewall prompt** for the bundled `freenet.exe`'s P2P network
  ports (separate from the loopback-only 7509/47021 API ports, which
  shouldn't need firewall approval).
- **WebView2 dependency** - no `bundle.windows.webviewInstallMode` is set in
  `tauri.conf.json`, so it uses Tauri's default (`downloadBootstrapper`):
  fetches WebView2 at install time if not already present. Nearly always a
  no-op on real Windows 10/11 machines (Microsoft ships it via Windows
  Update), but a locked-down/offline machine would need internet access
  during install specifically for this.
- **First-run identity publish can hit the documented gateway-network
  flakiness above** (observed directly during this work's own testing) -
  the UI surfaces a clean, retryable error rather than crashing, and simply
  retrying with the same passphrase succeeds (the identity file is already
  created locally by the first attempt, so a retry unlocks it rather than
  re-creating it - see `handle_unlock`'s `is_new` check, computed fresh from
  disk on every call).
- **No passphrase recovery** - `keys.rs`'s encryption is by design
  one-way; a forgotten passphrase means a lost identity, worth calling out
  explicitly to a non-technical recipient.

### Part 2: real in-app passphrase unlock, locked/unlocked startup split

`delegate/src/ipc.rs` gained a real `unlock { passphrase }` request and a
`lock_status` query (the only two requests answerable while locked); every
other request is refused with a clear "delegate is locked - send `unlock`
first" error until unlock succeeds. This needed restructuring startup, not
just adding a request type:

- `delegate/src/main.rs` no longer loads keys or touches Freenet at all -
  `main()` now just opens the local SQLite cache and calls
  `ipc::serve(IPC_PORT, db, identity_key_path)`, which binds and starts
  accepting connections immediately with `Delegate::unlocked: Option<Unlocked>`
  still `None`. Everything that used to run synchronously before the
  listener bound (connect to Freenet, publish/load this identity's
  `PublisherProfileContract`/`ContentIndexContract`, connect the NWC/platform
  fee wallets) moved into `ipc.rs`'s `finish_unlock`, which now runs once a
  passphrase actually arrives.
- **Two ways a passphrase arrives**, racing to unlock first (whichever gets
  the lock first wins, the other no-ops): (1) `try_legacy_auto_unlock`,
  spawned alongside the listener, reuses `DelegateKeys::load_or_generate`
  *completely unchanged* - same `AETHERIA_DEV_PASSPHRASE` env var check, same
  `rpassword` stdin prompt on a real interactive terminal
  (`std::io::stdin().is_terminal()`) - just no longer blocking the listener
  from starting first; if neither applies (no env var, no terminal - the
  real Tauri sidecar case), it's a no-op and the delegate just waits. (2) A
  real `unlock` IPC request from `UnlockScreen`.
- **New/existing identity distinction** happens purely by checking
  `identity_key_path.exists()` server-side (`handle_unlock`) - the caller
  doesn't have to say which case it thinks it's in.
  `DelegateKeys::create_new`/`unlock_existing` (new, non-interactive
  counterparts to the existing `load_or_generate`'s two branches - the
  original CLI/stdin functions are untouched) do the actual work; a wrong
  passphrase against an existing file surfaces as a plain, retryable `Err`
  from `unlock_existing`, not a crash.
- **`Delegate::unlocked()`/`unlocked_mut()`** panic if called while locked -
  safe because `handle_request`'s dispatch gate refuses every request except
  `Unlock`/`LockStatus` before any handler that calls them is ever reached.
- **Frontend**: `app/src/components/UnlockScreen.tsx` gates the entire app
  in `App.tsx` (before `Sidebar`/`RightRail`/anything else renders, earlier
  in the lifecycle than the pre-existing `FirstRunNamePrompt` overlay, which
  still runs afterward for a genuinely blank display name) - shows a plain
  single-field "Unlock" form if `lock_status.has_existing_identity`, or a
  passphrase+confirm "Create identity" form otherwise (confirmation is
  validated client-side, matching what the CLI's `prompt_new_passphrase`
  double-entry already enforces server-side for the legacy path).
  `app/src/lib/delegate.ts` gained `lockStatus()`/`unlock(passphrase)`.
- `AETHERIA_DEV_PASSPHRASE` is no longer hardcoded into the Tauri sidecar
  spawn in `app/src-tauri/src/main.rs` - `keys.rs` still honors it (and the
  interactive prompt) for CLI/dev use, it's just not baked into the shipped
  app's own launch path anymore.
- **Verified for real, 2026-08-02**, driving the actual release delegate
  binary through the real Vite dev server UI (not just unit tests), each
  against a fresh empty `AETHERIA_DATA_DIR_OVERRIDE` scratch dir:
  - Fresh dir, no terminal, no env var: delegate logged "delegate stays
    locked until a UI sends an `unlock` request" and bound its IPC listener
    immediately (no hang). Loading the UI showed the "Create identity" form.
    Submitting a passphrase + matching confirmation created a real encrypted
    identity, connected to the real live Freenet node, published real
    `PublisherProfileContract`/`ContentIndexContract` instances, and
    transitioned into the normal app (which then correctly showed the
    unrelated, pre-existing first-run display-name prompt, since the fresh
    identity has no name yet).
  - Killed and relaunched against the *same* now-non-empty scratch dir: UI
    correctly showed the plain "Unlock" form this time
    (`has_existing_identity: true`). A **wrong passphrase** produced the
    clean, in-UI, retryable error "wrong passphrase, or the identity file is
    corrupt" - delegate process stayed alive, form stayed usable, no crash.
    Retrying with the **correct** passphrase unlocked successfully and
    derived the exact same `content_index`/`publisher_profile` contract keys
    as the first run, confirming it loaded the same identity rather than
    creating a new one.
  - The legacy `AETHERIA_DEV_PASSPHRASE` env-var path against a separate
    fresh scratch dir: auto-unlocked with **no IPC call from the UI at
    all** - "AETHERIA_DEV_PASSPHRASE is set - using it instead of an
    interactive prompt" (the same log line as before this refactor, since
    that code is untouched) fired automatically from the background task.
    On this run the real network's PUT happened to exhaust all 4 retries
    (the documented gateway-network flakiness above, not a bug) - confirmed
    the delegate handled that honestly too: logged a clear
    "finishing startup after automatic unlock failed - delegate stays
    locked" and did *not* end up in a broken half-unlocked state.
  - The interactive stdin-prompt path (a real attached terminal) was not
    independently re-driven end-to-end in this pass, since `load_or_generate`
    and its prompt helpers in `keys.rs` are byte-for-byte unchanged by this
    refactor and were already exercised via the env-var path above through
    the identical `try_legacy_auto_unlock` call site - noted here rather than
    claimed as directly observed.

## Payments and subscriptions removed (as of 2026-08-05)

The user decided to drop payments/subscriptions entirely: Aetheria is now a
free, follow-only publishing platform. Everything below this note that used
to describe the NWC/Lightning subscription flow and the 2% platform fee (both
built and verified live against a real Bitcoin network, per this file's own
prior history) has been ripped out, not merely disabled:

- **Contracts**: `subscriber-registry-contract` crate deleted;
  `AccessTier`/`Tier`/`EncryptedKeyBundle` removed from `aetheria-types`;
  `PostMetadataHeader`/`GlobalDirectoryEntry` no longer carry `access_level`/
  `epoch_id`; `EncryptedPostPayload` renamed to `PostPayload` (plain
  `content: Vec<u8>`, no `nonce`/`auth_tag` - every post is public now, so
  there's no plaintext-vs-ciphertext distinction to encode). **All four
  remaining contracts were rebuilt with `fdev build`, so their code hashes
  changed** - this is a real, unavoidable consequence: every contract
  instance key derives from `(code, params)`, so this machine's
  previously-published `PublisherProfileContract`/`ContentIndexContract`/
  `PostDataContract`/`GlobalDirectoryContract` instances (all the keys this
  file documented earlier) are now unreachable under the new code. The next
  publish mints fresh instances at new keys; nothing tries to migrate the old
  ones.
- **Delegate**: `crypto.rs` (ECDH/epoch-key AES-GCM), `nwc.rs` (NIP-47
  client), `subscriber_registry_e2e_test.rs`, and `examples/`
  (`mock_nwc_wallet.rs`, `nwc_protocol_check.rs`) all deleted outright.
  `contracts.rs` lost `subscriber_registry_key_for`/`ensure_subscriber_registry`/
  `publish_key_bundle_to_network`/`fetch_key_bundle`. `ipc.rs` lost
  `ConnectWallet`/`GetSubscriptionInfo`/`Subscribe`/`ListSubscribers` and the
  `access` field on `PublishPost`; every feed handler's `locked`/
  `access_level`/`epoch_id` fields are gone (every post is just public now).
  `db.rs` lost the `epoch_keys`/`subscribers` tables and their columns on
  `posts`/`cached_remote_posts`. `nwc`/`nostr-sdk` dropped from
  `Cargo.toml`; `hkdf`/`sha2` too (only `crypto.rs` used them). `k256`/
  `aes-gcm` stay - `keys.rs`'s on-disk encrypted-identity format still
  includes a (now otherwise-unused) secp256k1 key, kept rather than forcing
  a breaking migration of this machine's real identity file for no benefit
  (see `keys.rs`'s module docs).
- **Frontend**: `SubscriberPortal.tsx` and `Subscriptions.tsx` deleted;
  Sidebar's Subscribers/Subscriptions nav entries gone. `Editor.tsx` lost the
  public/subscriber-only toggle - publishing is just title/summary/markdown
  now. `FeedItemsList.tsx`/`RightRail.tsx` lost their lock-badge rendering.
  Since a publisher's pubkey used to be shown on the now-deleted Subscribers
  tab (needed so someone else can paste it into Following), `Profile.tsx`
  gained that display instead (`ipc.rs`'s `handle_get_profile` now returns
  `author_pubkey`) - `Following.tsx`'s instructional text was updated to
  point there.
- **Local SQLite**: schema changed (see above) but existing on-disk dev
  databases are **not** migrated - this is genuinely dev-only data on a
  single-user machine, and writing a migration for columns nothing reads
  anymore isn't worth it. A fresh `aetheria.sqlite` (delete
  `%APPDATA%\aetheria\aetheria-delegate\data\aetheria.sqlite`) is the clean
  path if the old schema's leftover `NOT NULL` columns ever cause an insert
  to fail.
- **Verified**: `cargo check`/`cargo test --lib` clean across both
  `contracts/` and `delegate/` (16 non-network tests passing, 4 correctly
  `#[ignore]`d), `cargo build --release` clean, `npx tsc -b` clean, and all
  four contracts rebuilt via `fdev build`. The website (`website/`) was
  updated in the same pass to drop every payment/subscription mention - see
  that directory's own history/CLAUDE.md.

## Following other publishers + merged Home/Following feeds (as of 2026-08-02)

A reader can now follow another publisher by pasting their Ed25519
`author_pubkey` (hex) and see a real merged feed of "your own posts + every
followed publisher's posts", sorted by recency. This works with **no
discovery service at all** - the same pure-local-hash trick
`subscriber_registry_key_for` already used (see that function's module docs
in `contracts.rs`): `ContractKey::from_params_and_code(params, code)` is a
deterministic hash of `(compiled contract code, Parameters)`, so any delegate
holding the same compiled `PublisherProfileContract`/`ContentIndexContract`
WASM and a publisher's `author_pubkey` can independently derive that
publisher's exact contract keys and `FreenetBridge::get_state` them directly.

- **New `contracts.rs` functions** (all read-only, network-only, no local DB
  dependency - same style as `fetch_key_bundle`): `fetch_remote_profile`
  fetches and *verifies* (Ed25519 `verify_strict` against the header's own
  `author_pubkey`) a remote `PublisherProfileContract` before returning
  anything, so a caller never saves a follow for an unverified/tampered
  profile; `fetch_remote_posts` fetches a remote `ContentIndexContract` and
  drops (logs, doesn't fail the whole fetch) any individual
  `PostMetadataHeader` whose signature doesn't check out; `fetch_remote_post_payload`
  fetches a specific `PostDataContract` instance by its encoded contract id.
- **Parsing an encoded contract id back into something `FreenetBridge::get_state`
  accepts**: `freenet_stdlib::prelude::ContractInstanceId::from_base58(&str)`
  (found by reading `freenet-stdlib-0.8.5`'s own
  `src/contract_interface/key.rs` in the cargo registry cache, the same
  research method used for the WebSocket URL/encoding issues earlier in this
  file) - the exact inverse of `ContractKey::encoded_contract_id()`, which
  every other write path in `contracts.rs` already produces. Notably,
  `FreenetBridge::get_state` only ever needs a `ContractInstanceId`, never
  the full `ContractKey` (code hash included) - so no code hash needs to be
  recovered to GET a remote post by id, only to construct a key for PUT/UPDATE
  (which a reader never does for someone else's contract anyway). The crate
  also exposes a now-deprecated `from_bytes` alias and a `FromStr`/`TryFrom<String>`
  impl that both delegate to `from_base58` - the source's own doc comments
  warn at length that this parses base58 *text*, not raw 32-byte ids (a
  previously-real bug elsewhere in the ecosystem confused the two).
- **New SQLite table** `followed_publishers` (`db.rs`): `author_pubkey BLOB
  PRIMARY KEY`, cached `display_name`/`avatar_freenet_key` for fast
  rendering, `followed_at`. A brand-new table (like `profile`), so a plain
  `CREATE TABLE IF NOT EXISTS` was enough - no `ALTER TABLE` migration guard
  needed. `follow_publisher` upserts (re-following refreshes the cached
  name), `unfollow_publisher` deletes, `list_followed_publishers` reads all,
  ordered most-recently-followed first.
- **New IPC ops** (`ipc.rs`): `follow_publisher { author_pubkey }` (hex) -
  calls `fetch_remote_profile` first and fails clearly if nothing is found
  (or if `author_pubkey` is this delegate's own - "you're already in your
  own Home feed"), only then saves; `unfollow_publisher { author_pubkey }`;
  `list_followed_publishers` (local-only, no network call);
  `get_home_feed` (this delegate's own posts, from local SQLite, merged with
  every followed publisher's posts, fetched live and best-effort per
  publisher - one followed publisher's fetch failing is logged and skipped,
  not treated as a reason to fail the whole feed, same "real gateway network
  is flaky, don't propagate that as a hard error" philosophy as everywhere
  else in this file - sorted by `published_at` descending);
  `get_following_feed` (same merge, followed-only, backs the Following tab);
  `get_remote_post { post_contract_id }` (opens a `Public` post from another
  publisher - refuses, with a clear error, if the fetched payload's nonce
  turns out to be non-zero, i.e. it was actually `SubscriberOnly` all along;
  re-checks independently rather than trusting the caller's claim).
- **Frontend**: a new "Following" tab (`app/src/components/Following.tsx`) -
  paste-a-pubkey input (there's no directory to browse, per design), list of
  followed publishers with Unfollow, and the followed-only feed below it.
  `ReaderFeed.tsx`'s Home view now renders `get_home_feed`'s merged feed
  instead of just local posts. Both feeds share `FeedItemsList.tsx` (per-card
  author name/avatar-initial/timestamp/locked-badge rendering) and
  `OpenedPostView.tsx` (the "reading a single post" screen) rather than
  duplicating that chrome. A `subscriber`-access post from someone *other*
  than this delegate renders with a lock icon and a disabled open button
  (see the gap below for why) - from this delegate's own identity, the same
  access level renders as a normal openable "Subscriber" badge, unchanged
  from before.
- **Verified live, 2026-08-02**: `delegate/src/follow_publisher_e2e_test.rs`
  (same shape and rigor as `subscriber_registry_e2e_test.rs` - two genuinely
  independent identities, `#[ignore]`d, needs a live node, run with `cargo
  test follow_publisher_e2e -- --ignored --nocapture`) mints a second, real,
  clearly-test-labeled publisher identity, publishes a real public post for
  it, then as a completely independent reader identity follows it (verifying
  the real signed profile), fetches its real post index, and recovers the
  *exact* markdown over the real network. Independently re-confirmed with
  `fdev -p 7509 execute get <post-contract-id>` from a separate shell (same
  methodology as every other network-verification note in this file) - raw
  CBOR bytes contained the literal published markdown. Also drove the entire
  feature through the real running delegate binary and the real browser UI
  (Vite dev server on 5173): followed that same real test-publisher pubkey
  via the Following tab's input, confirmed their profile name and public
  post appeared and opened correctly, confirmed their post appeared merged
  into the Home feed sorted correctly by recency, confirmed following a
  nonexistent pubkey and this delegate's own pubkey both fail with clear
  in-UI error messages and save nothing, and (via a throwaway second test
  identity publishing one `SubscriberOnly` post, followed the same way, then
  unfollowed again afterward to avoid leaving test data in the real local
  DB) confirmed a subscriber-only post from another publisher renders locked
  with a disabled open button rather than attempting - and failing - a
  decrypt.

## Home = following-only feed, Latest = network-wide feed, real profile pages (as of 2026-08-03)

The user's own framing, for context: Home used to feel "basically like the
profile view" - on an account following nobody, `get_home_feed`'s own+
followed merge degenerated to just your own posts. Redesigned as:

- **Home** (`get_following_feed`, unchanged from before) - posts from people
  you follow, and *only* those - no own-post merge anymore.
- **Latest** (new) - the most recent posts from *every* publisher on the
  network, own included, via a brand-new shared contract (below). This is
  the actual "discover people you haven't followed yet" surface; there was
  no way to do this at all before (see the "Known stub" bullet this
  replaces).
- **Following tab** - trimmed to pure follow-management (paste-a-pubkey box
  + followed list); the feed itself moved to Home, so showing it twice was
  redundant.
- **Publisher profile pages** (new, `PublisherProfileView.tsx`) - clicking
  any non-own author's name in *any* feed (Home, Latest, or an opened post)
  now opens their real profile (network-fetched and signature-verified, same
  `fetch_remote_profile` Following already used) with a Follow/Unfollow
  button - the more common way to follow someone now, vs. Following's
  paste-a-pubkey box (kept for the case where you don't have a post of
  theirs to click yet).
- **Subscribers vs. Subscriptions** - this originally split a dual-purpose
  `SubscriberPortal.tsx` into a publisher-side "Subscribers" tab and a
  reader-side "Subscriptions" tab. Both tabs, and the wallet-connect/pay UI
  behind them, were deleted when payments/subscriptions were removed (see
  "Payments and subscriptions removed" above) - a publisher's own pubkey
  (needed so others can paste it into Following) now lives on the Profile
  tab instead.

### GlobalDirectoryContract - the "everyone" list

Not in the design doc - there was never a spec for network-wide discovery
(checked the PDF directly before building this: contracts are `PublisherProfile`
/ `ContentIndexContract` / `SubscriberRegistryContract` / `PostDataContract`,
nothing else). `contracts/global-directory-contract/` (new crate, mirrors
`content-index-contract`'s structure/CRDT-merge-by-`post_id` shape) is a
single, well-known-key, globally-shared contract every publisher's delegate
appends to on every successful publish (`ipc.rs::handle_publish_post`, via
`contracts::publish_to_global_directory` - best-effort, same "don't fail the
whole publish over this" philosophy as everything else in that function).

- **Deterministic singleton key**: `contracts::global_directory_key()` uses
  **empty** `Parameters` (every other contract in this app scopes params to
  a publisher's pubkey) - so unlike everything else here, this isn't "any
  delegate can derive *this publisher's* key", it's "every delegate derives
  the exact same one key for the whole network", with no discovery/pointer
  field, same `ContractKey::from_params_and_code` trick used everywhere else
  in `contracts.rs`.
- **Bootstrap**: `publish_to_global_directory` GETs first; if nothing exists
  yet it PUTs a fresh one, otherwise it UPDATEs. `put_new` against an
  already-existing key is unexplored territory in this codebase (every other
  contract here has exactly one authoritative publisher, so this never came
  up before) - checking first narrows but doesn't eliminate the race if two
  delegates bootstrap simultaneously; left to Freenet's own handling for that
  case, since the loser's *next* publish still merges its entry in via the
  update path regardless.
- **Capped at 1000 entries** (`GLOBAL_DIRECTORY_MAX_ENTRIES` in
  `contracts.rs`, `MAX_ENTRIES` in the contract crate - keep these two in
  sync by hand, same caveat as every other hand-mirrored state struct in this
  file), newest-first, oldest evicted on merge - the user's own suggestion,
  and the closest thing this app has to design doc §7's Sybil-spam
  mitigation (still no real proof-of-work/payment gate).
- **Per-entry signatures**: unlike `ContentIndexContract` (one publisher, one
  verifying key for the whole state), this contract holds entries from many
  different authors, so each `GlobalDirectoryEntry` carries its own
  signature, checked independently against its own `author_pubkey` by
  `contracts::fetch_global_directory` - a bad/tampered entry is dropped
  (logged), not treated as a reason to distrust every other real entry.
- **Locked-post teasers**: a `SubscriberOnly` post from someone else still
  appears in Latest (and Home, and a profile page) with its real title/
  summary and a lock badge, exactly like Following already did - this was
  the user's explicit ask ("still see that a subscriber-only post was made,
  just not the details, to entice a subscribe"), and it turned out to
  already be exactly how the existing `locked` flag worked; the only
  backend change needed was applying the same `is_own`-gated rule
  (`ipc.rs::feed_item_json`, factored out of the old per-feed-handler
  duplication) to the new feeds too.

**Verified live, 2026-08-03**: two genuinely independent identities (fresh
scratch `AETHERIA_DATA_DIR_OVERRIDE` dirs, real release delegate binary, real
running Freenet node) driven sequentially through the real IPC protocol by a
script at the same rigor as `follow_publisher_e2e_test.rs` (not checked into
the repo - throwaway verification tooling): Alice published a public and a
subscriber-only post; Bob published a public post; Bob's `get_latest_feed`
showed all three with correct `locked`/`is_own` flags and newest-first sort;
`get_publisher_profile` returned Alice's real signed profile + posts, `is_following: false`
before and `true` after `follow_publisher`; Bob's `get_following_feed` showed
only Alice's two posts (subscriber one still locked); `subscribe` to Alice's
pubkey rejected immediately with the documented error; `unfollow_publisher`
correctly emptied the following feed again. Also driven through the real
browser UI (Vite dev server) against a real delegate: Home empty until
following someone, Latest showing real cross-identity data with correct
locked/You badges (including correctly treating two *different* real
identities that happened to share a display name as distinct, since the
locked check is keyed on pubkey, not name), clicking an author's name
opening their real profile, Follow working live and Home updating to match,
Following tab showing management-only (no duplicate feed), Subscriptions
showing a real wallet-connect flow and an honest per-publisher error, and
Subscribers showing publisher-side-only content with no wallet/Subscribe UI
left on it.

## Real search bar (as of 2026-08-03)

`RightRail.tsx`'s "Search Aetheria" box used to be static markup with no
`<input>` at all. Real now, client-side only - no new IPC op, no server-side
index. Searches over the same two sources everything else in the app
already treats as "what's reachable": the Latest feed (`get_latest_feed` -
every publisher's recent posts network-wide, up to its 1000-entry cap) for
post title/summary/author matches, plus `list_followed_publishers` for
publisher-name matches from people you follow but haven't necessarily
posted anything findable yet. Debounced 300ms. Selecting a post result
resolves it via a new shared `app/src/lib/feedItem.ts::openFeedItem` helper
(factored out of what were three separate copies of the same is_own
branch in `ReaderFeed.tsx`/`PublisherProfileView.tsx`/here) and opens it at
the `App.tsx` level via a new `searchOpenedPost` state, independent of
whatever tab you're on; selecting a publisher result reuses the existing
`viewingAuthor` navigation Following/feeds already use. A locked
(subscriber-only, someone else's) result still shows up with a lock badge,
same convention as every other feed - just not openable.

## Marketing website (as of 2026-08-03)

`website/` - a separate Next.js (App Router, TypeScript, Tailwind v4) site
for non-technical visitors: what Aetheria is, download links, plain-
language docs, and a read-only view of real posts. Deployed independently
of the desktop app - the user connects this repo to a Vercel project
themselves and points a domain (registered via Wix, DNS pointed at Vercel)
at it; **Vercel's project settings need Root Directory set to `website/`**
since this is a monorepo. See `website/CLAUDE.md` for details scoped to
that subproject; this section covers the cross-cutting pieces.

- **Downloads are served directly from the site, not GitHub Releases** -
  the user's explicit call. `website/public/downloads/` holds
  `Aetheria-Setup-x64.exe` (the existing NSIS installer, bundles Freenet -
  copy of `builds/Aetheria_0.1.0_x64-setup.exe`) and
  `Aetheria-app-only-x64.zip` (new packaging: just `aetheria.exe` +
  `aetheria-delegate.exe`, zipped, no bundled Freenet - for people already
  running their own node). Both are committed to git (unusual for this repo
  - everywhere else built binaries are gitignored, see the `scripts/`
  section above - but there's no other artifact host in this plan, so
  Vercel can only serve what's actually in the repo). Re-run
  `scripts/build.sh` then re-copy/re-zip into `website/public/downloads/`
  before any release that should reach this download page.
- **The `/latest` page can't hold a live Freenet connection** - Vercel
  serverless functions are stateless/ephemeral, and a fresh Freenet node
  needs real time to get P2P ring connections (see this file's environment
  notes on the real network being flaky/slow to connect), which doesn't fit
  a per-request function. Solved with a periodic static snapshot instead of
  a live connection - a real, explicit tradeoff discussed with the user
  (the alternative, a genuinely live view, needs an always-on backend
  server outside Vercel, real hosting cost and ops, not just git-push
  deploys - deferred unless that tradeoff needs revisiting later):
  - `delegate/src/bin/snapshot_latest_feed.rs` (new `[[bin]]` target,
    reuses `contracts::fetch_global_directory` via the delegate crate's new
    library split above - no keys, no writes, purely a GET against the
    same shared `GlobalDirectoryContract` the app's own Latest tab reads)
    dumps the current entries as JSON with a `generated_at` timestamp.
  - `website/app/latest/page.tsx` is a Server Component that reads
    `website/public/data/latest-feed.json` straight off disk
    (`fs.readFileSync`, no client-side fetch, no API route) and shows an
    honest "snapshot updated <time>" line rather than implying it's live.
  - `.github/workflows/refresh-latest-feed.yml` runs on a 30-minute cron
    (plus manual `workflow_dispatch`): installs `freenet`+`fdev` via
    `cargo install` (cached across runs), runs `fdev build` for every
    contract the snapshot tool's `include_bytes!`s need (same
    `CARGO_TARGET_DIR` workaround this file already documents for `fdev
    build`'s workspace-root bug - applies identically on a CI runner, not
    just this dev machine), starts a real `freenet network` node, waits for
    it to bind and get some peer time, runs the snapshot tool, and commits
    the JSON if it changed. A push to `main` is what actually refreshes the
    live page, via Vercel's normal git-push deploy - this workflow's commit
    is the trigger, not a separate deploy step.
  - **Honesty note**: written and logic-reviewed carefully (matches every
    documented gotcha about `fdev build`/cold Freenet nodes elsewhere in
    this file), but a real run on GitHub's own infrastructure has not been
    observed from this environment - no `gh` CLI is installed here (see
    environment notes above), so there's no way to trigger/inspect an
    Actions run directly. Check the Actions tab after this first merges to
    confirm it goes green, especially the node-connectivity timing, which
    is the one piece genuinely outside this session's ability to verify.
  - To refresh by hand instead: from `delegate/`, run
    `cargo run --release --bin snapshot-latest-feed > ../website/public/data/latest-feed.json`
    against a real reachable Freenet node, then commit the file.
- **Verified locally, 2026-08-03**: `snapshot-latest-feed` run against the
  real local node produced real current data (matched what the app's own
  Latest tab shows); `npm run build` in `website/` succeeds cleanly, all
  routes statically prerendered; loaded `/`, `/download`, `/docs/security`,
  and `/latest` in a real browser against the Next.js dev server - `/latest`
  rendered the real snapshot with correct locked/teaser rendering for
  subscriber-only posts (title/summary visible, content withheld, same
  convention as the app itself), and both download links resolved with
  real, correct `Content-Length`s (not 404s or placeholders).

## Live Freenet connectivity indicator (as of 2026-08-04)

Written after a long debugging session where "why isn't Aetheria connecting
to Freenet" took hours, and the real causes turned out to be a leftover
process squatting port 7509, a stale bundled Freenet binary that needed an
auto-update it couldn't apply itself, and finally **NordVPN routing all P2P
traffic through a tunnel that broke NAT hole-punching entirely**. Every one
of those produced the *identical* visible symptom: feeds just looked empty,
with nothing anywhere in the UI indicating the node had zero real peer
connections. A real end user hitting any of them - the VPN case especially,
which is common - would conclude the app is broken or that the network is
simply empty, with no way to tell those apart.

Before this, the only connectivity indicator in the entire app was for the
NWC Lightning wallet (`wallet_connected`, rendered in `Subscriptions.tsx`) -
nothing at all for Freenet itself, which is both more fundamental and far
more likely to silently fail.

### There *is* a real node-status query - this is not an inference

The important finding, and the reason this feature reports something
trustworthy rather than a guess: `freenet-stdlib`'s client API **does**
expose a direct node-diagnostics query, over the exact same
`ws://127.0.0.1:7509/v1/contract/command?encodingProtocol=native` socket
`FreenetBridge` already holds. Found by reading the crate source in the
cargo registry cache (`freenet-stdlib-0.8.5/src/client_api/client_events.rs`),
the same research method this file already documents for the WebSocket
URL/encoding and `ContractInstanceId::from_base58` questions:

- `ClientRequest::NodeQueries(NodeQuery::NodeDiagnostics { config })` →
  `HostResponse::QueryResponse(QueryResponse::NodeDiagnostics(..))`, carrying
  `NetworkInfo { connected_peers, active_connections }` and
  `NodeInfo { peer_id, .. }`. `NodeDiagnosticsConfig::basic_status()` asks
  for node info + network info only - deliberately not subscriptions,
  contract states, or per-peer detail, since this runs every few seconds and
  those make the node clone its full contract/subscription maps.
- Confirmed the **node side** handles it too, not just that the type exists:
  `freenet-0.2.119/src/client_events.rs` routes it to
  `NodeEvent::QueryNodeDiagnostics`, and
  `src/node/network_bridge/p2p_protoc.rs` answers it by walking
  `op_manager.ring.connection_manager.get_connections_by_location()` and
  deduplicating. **That is the same connection map the node's own
  `ring_connections=N` / "Node isolated with zero ring connections" log lines
  are computed from** - so this reports exactly what those logs report,
  over the API instead of by scraping a log file. 0.2.119 is the version
  bundled in `%LOCALAPPDATA%\Aetheria\freenet.exe` on this machine.
- Because it's a **local, in-process** question for the node (it inspects its
  own connection table; no gateway routing involved), `query_node_status`
  deliberately has **no retry loop**, unlike every other method in
  `freenet_bridge.rs`. There's no flaky remote hop that could make a single
  attempt spuriously fail, which is exactly why a zero answer can be trusted
  rather than written off as network flakiness.

So: the direct-query path, not the infer-from-recent-operation-outcomes
fallback. The operational signal below exists too, but as a genuinely
*separate second* signal, not as a substitute.

### What was built

- **`delegate/src/freenet_bridge.rs`**: `query_node_status() -> NodeStatus`
  (never returns `Err` - the whole point is reporting how broken things are,
  so a failed query is data, not a caller-facing failure), plus an `OpHealth`
  struct tracking `last_success`/`last_error` across every
  `put_new`/`update_state`/`get_state`. Notes worth keeping:
  - **`NODE_QUERY_TIMEOUT` (5s) is not optional.** `ipc.rs::handle_message`
    holds the single global `Mutex<Delegate>` for the entire duration of
    every request, so a `recv()` that never returns would wedge *every* other
    IPC request - the whole UI, not just this indicator. A node that accepts
    `NodeQueries` but never answers is precisely the failure this feature
    exists to surface, so it has to be survivable. A late response is
    harmless: it stays buffered and the next contract operation's recv loop
    discards it through its existing "ignoring unrelated host response" arm.
  - Only failures that **exhausted all `MAX_ATTEMPTS` retries** are recorded
    as `last_error`; single transient attempt failures are already expected
    on this network (see the environment notes above) and recording them
    would make the signal noise.
  - `get_state` returning `Ok(None)` (contract not found) counts as a
    **success** - it's a real, complete answer from the node, and this signal
    measures whether the node is answering, not whether a contract exists.
  - `OpHealth` is a `std::sync::Mutex`, not a `tokio` one: it's held for two
    field assignments with no `.await` between, so an async mutex would only
    add a suspension point to every contract operation.
- **`delegate/src/ipc.rs`**: `get_network_status`, the **third** request
  answerable while locked (with `unlock`/`lock_status`). Returns
  `{ state, freenet_connected, peer_count, node_peer_id,
  last_successful_operation_secs_ago, last_error, query_error }`. `state` is
  the one field a UI should switch on:
  - `connected` - node reports ≥1 peer.
  - `isolated` - node is up and answering but has **zero** peers. Feeds look
    empty, nothing publishes. The VPN/firewall state; the reason this exists.
  - `unknown` - the node didn't answer the query at all (`query_error` says
    why). Usually the node process died or its API socket dropped.
  - `locked` - see below.
- **`app/src/components/NetworkStatusPanel.tsx`** (new), rendered by
  `RightRail.tsx` directly above the pre-existing "Local Delegate" block, so
  it's persistently visible on every tab rather than buried in Settings.
  Polls every 5s. Kept as its own component rather than inlined because
  `RightRail.tsx` is already 200+ lines of unrelated search logic. State
  descriptions live in a `describe()` lookup rather than nested JSX ternaries
  specifically so no state can silently fall through to a default that
  *overstates* connectivity. An `inFlight` ref skips a tick rather than
  piling up requests (the delegate serializes all IPC behind one lock, so a
  poll can legitimately queue behind a long feed fetch), and a failed poll
  sets an "unreachable" flag without clearing the last known status, so a
  delegate hiccup doesn't flash the panel back to "Checking…".
- **`app/src/lib/delegate.ts`**: `getNetworkStatus()` + `NetworkStatus` type,
  matching the existing typed-client patterns.

**Judgment call on the locked state** (point 4 of the task): the Freenet
connection genuinely does not exist until after unlock - `finish_unlock`
is what builds the `FreenetBridge`, and `Unlocked` exists precisely because a
bridge has no meaningful empty state. Rather than force a connection to
exist earlier than it structurally can, `get_network_status` is answerable
while locked and reports `state: "locked"`, which is the truthful answer:
there's no connection *because nothing has unlocked one yet*, which is
different from one being broken. The UI only renders the panel after unlock
anyway, so in practice that branch serves scripts and any future pre-unlock
diagnostic screen.

**Why `last_successful_operation_secs_ago`/`last_error` are kept alongside
the peer count rather than folded into `state`**: they can disagree in an
informative way. A node with healthy peer connections whose operations are
all still timing out is the documented gateway-network flakiness, not a
connectivity problem, and the panel says so with a separate amber line
instead of contradicting its own headline.

### Verified live, 2026-08-04

Real scratch `freenet` node(s) + the real delegate binary against a scratch
`AETHERIA_DATA_DIR_OVERRIDE` identity, driven both through the real IPC
socket (Node scripts) and through the real browser UI (Vite dev server on
5173, DOM asserted directly for both text and the status-dot class):

- **`connected`** - real public network, node reporting **43 peers**; UI
  showed a green dot and "Connected — 43 peers". Peer count tracked the live
  network genuinely growing over a single session (28 → 38 → 43, and on a
  separate cold start 8 → 9 → 10 → 11 across successive polls).
  `last_successful_operation_secs_ago` ticked 40 → 42 → 44 across polls
  spaced exactly 2s apart, confirming it's real elapsed time from the real
  contract publish at startup, not a fabricated value.
- **`unknown`** - killed the Freenet node process out from under a running
  delegate with the UI open and untouched. Within the 5s polling interval
  and **with no page reload**, the panel flipped on its own from green
  "Connected — 43 peers" to a red dot and "Can't reach your Freenet node",
  showing the real underlying error ("sending node diagnostics query:
  unhandled error: client error: comm channel between client/host closed").
  This is the single most important behaviour of the feature and it was
  observed directly, not reasoned about.
- **`isolated`** - reproduced two independent ways. (1) Caught the genuine
  zero-peer window of a cold network-mode node by starting the delegate
  *first* (its `connect_local` retry attaches the instant the node binds) and
  polling every 250ms: a real `state=isolated peers=0` held for ~2.4s before
  transitioning to `connected peers=1`. (2) For a *sustained* window a node
  was run with `--skip-load-from-network --gateway
  "127.0.0.1:31399,<bogus-key>"` - explicit `--gateway` CLI entries replace
  the on-disk cache, so the node comes up healthy, serves its API, and can
  reach nobody: a faithful stand-in for the VPN/firewall case. Against it the
  IPC op returned `state: "isolated", peer_count: 0` steadily, and the UI
  rendered an amber dot with "No peer connections" and the plain-language
  VPN/firewall hint.
  - Note: **`gateways.toml` cannot be used for this** - freenet rewrites it
    with the real default gateways on every startup (tried both blanking it
    and pointing it at an unreachable host; both were overwritten). The
    `--skip-load-from-network` + `--gateway` CLI combination is the way.
- **`freenet local` mode is not a zero-peer node** - it answers the
  diagnostics query with "not supported" (local mode has no P2P network
  bridge to service `QueryNodeDiagnostics` at all), which correctly surfaces
  as `unknown` with that error text rather than crashing or being mistaken
  for `isolated`. Two distinct real causes both landing in `unknown` with
  distinguishable messages.
- **`delegate/src/network_status_e2e_test.rs`** (new, `#[ignore]`d, same
  shape as the other two e2e tests - run with
  `cargo test network_status_e2e -- --ignored --nocapture`). Deliberately
  does **not** assert `peer_count > 0` - a cold or VPN-blocked node
  legitimately reports zero and that's the state the feature exists for.
  It asserts the node *answered*: `peer_count.is_some()` and
  `query_error.is_none()`, only possible if the query really round-tripped.
  A second test drives a real `get_state` and asserts the operational signal
  moved off `None`. Both branches of that second test were exercised for
  real across runs: the success branch against the healthy node, and the
  failure branch against the isolated node, where the GET failed with the
  node's own honest "peer has not joined the network yet" and was correctly
  recorded as `last_error`.
- `cargo test` (15 passed, 4 ignored) and `npx tsc -b` both clean;
  `cargo build --release` clean.

### Known gaps / follow-ups

- ~~**`FreenetBridge` has no reconnect logic**~~ - fixed, see "Freenet
  reconnect + single-instance enforcement" below. This gap was flagged here
  as a known follow-up and then actually hit for real on a live install
  within the same day.
- The peer count is the node's **ring connection count**, which is about
  whether this node is meaningfully embedded in the network. It is not a
  guarantee that any particular GET/PUT will succeed - that's what the
  separate operational-health line is for. Neither signal is a promise, and
  the UI wording avoids implying one.
- No history/sparkline - the panel shows current state only. Nothing tracks
  how long a node has been isolated, which would make "cold start, wait a
  minute" vs. "your VPN is breaking this" distinguishable automatically
  rather than by the hint text listing both.
- The panel polls unconditionally while mounted, including when the window
  is in the background. At one 5s local query it's negligible, but it's not
  paused on `visibilitychange`.

## Real desktop notifications: "someone you follow published" (as of 2026-08-04)

Until now, "notify your subscribers" (the actual pitch this app is built
around) was aspirational - every feed in `ipc.rs` is a pull, and a reader
only learned about a new post by having the app open and hitting Refresh.
This closes that gap for real, with no new server, relay, or mailer: the
only moving parts are the user's own delegate and the Freenet node it
already talks to.

- **`FreenetBridge::subscribe` is real now** (`delegate/src/freenet_bridge.rs`,
  previously the `todo!()` the "Known stub" section below used to describe).
  Sends a real `ContractRequest::Subscribe` and returns the node's own
  `subscribed` flag - `false` means the node accepted the request but isn't
  watching that contract (typically nobody on the network is currently
  hosting it), a real, reportable outcome distinct from an error. A new
  `next_update_notification()` blocks for the next `UpdateNotification` push
  on the connection. There is no `unsubscribe` - `ContractRequest` (freenet-
  stdlib 0.8.5) has no such variant, so the only way to stop receiving a
  publisher's pushes is to drop the connection and rebuild it from the
  current follow list.
- **`delegate/src/watcher.rs`** is the new module that actually consumes
  this: for every followed publisher, it subscribes to their
  `ContentIndexContract` (the same key `contracts::fetch_remote_posts`
  already GETs - `content_index_key_for` is now `pub` so both call sites
  derive the identical key), on a **dedicated second `FreenetBridge`
  connection** rather than the one `ipc.rs` uses for requests - every other
  method on that bridge is a strict request/response round trip that
  discards anything else arriving mid-flight, so a push landing during an
  unrelated GET would simply be lost on the shared connection.
  - **Priming**: the first time a publisher is followed (or the app starts),
    their existing posts are absorbed silently rather than announced -
    otherwise following someone with forty old posts, or just restarting the
    app, would fire forty toasts at once. Only a post that shows up *after*
    priming is news.
  - **Push + poll, one claim**: a live subscription push is what makes this
    feel instant, but the real gateway network is documented throughout this
    file as flaky, and a subscription is exactly the kind of thing it can
    quietly drop. So `watcher.rs` also polls every followed publisher's index
    every 3 minutes as a backstop. Both paths funnel through
    `LocalStore::claim_post_notification` (new `notified_posts` table, an
    atomic `INSERT OR IGNORE`), so the two can never double-toast one post.
  - A pushed `ContentIndexState` gets exactly the same Ed25519 verification
    as a fetched one (`contracts::decode_verified_content_index`, factored
    out of `fetch_remote_posts` for this reason) - nothing about arriving
    unsolicited makes a pushed state more trustworthy.
- **Reaching the UI**: `ipc.rs` gained a real server-push channel over the
  *same* IPC WebSocket every request/response already uses - a push carries
  an `"event"` field and no `"id"`, which is how `app/src/lib/delegate.ts`
  (a new `on(event, handler)` subscription API, plus its own reconnect loop
  so a listening UI survives a delegate restart) tells it apart from a
  reply. `app/src/lib/notifications.ts` turns a `new_post` event into a real
  OS toast via a new `show_notification` Tauri command
  (`app/src-tauri/src/main.rs`, `tauri-plugin-notification`) - a subscriber-
  only post from someone else is still announced (the teaser is the point)
  but says so, since it can't be opened yet (see the ECDH gap below).
- **The app now lives in the system tray.** Notifications only matter if the
  app can still be listening when its window isn't in front of you, so
  closing the window now hides it to the tray (`tauri::tray`, `tray-icon`
  Cargo feature) instead of quitting - "Quit Aetheria" in the tray menu (or
  a left-click to reopen) is the one thing that actually exits, running the
  exact same sidecar cleanup as before. Standard Slack/Discord-style
  behavior, not a novel pattern.
- **Two dev/test escape hatches**, same spirit and same "unset for any
  normal run" rule as this file's existing ones: `AETHERIA_IPC_PORT`
  (`delegate/src/main.rs`) runs a delegate's IPC listener on a port other
  than 47021, and `AETHERIA_FREENET_WS_URL` (`freenet_bridge.rs`) points a
  delegate at a Freenet node on a port other than 7509. Both exist because
  verifying "your followers get notified" inherently needs *two* delegates
  (and, for a truly isolated test, two nodes) running on one machine at
  once, which the previously-hardcoded ports made impossible.
- **Verified live, 2026-08-04**:
  `delegate/src/new_post_notification_e2e_test.rs` (same shape and rigor as
  `follow_publisher_e2e_test.rs` - two genuinely independent identities,
  `#[ignore]`d, needs a live node, run with `cargo test
  new_post_notification_e2e -- --ignored --nocapture`) mints an independent
  publisher identity, publishes a backlog post, has a real `ipc::serve`
  reader instance (driven over a real WebSocket, not by calling handlers
  directly) follow them, then publishes a brand-new post and waits for it to
  arrive as an unprompted push over the real IPC socket. Passed with the
  push arriving **0.0s** after publishing (the real subscription firing, not
  the 3-minute poll fallback), correctly skipped the pre-follow backlog post,
  and carried the correct title/author/pubkey/locked flag. All 16 non-
  network unit tests pass, `tsc` is clean, and a full `npm run build:desktop`
  (including the new tray-icon and notification-plugin dependencies)
  succeeds with no warnings and produces both installers. The
  frontend-to-toast link itself (clicking through an actual installed,
  packaged build and confirming a real Windows toast appears) has **not**
  been independently re-driven end-to-end - noted here rather than claimed,
  since that specific link needs a real desktop session to observe and is
  the one piece of this feature a live subscription-network test can't
  reach.

## Freenet reconnect + single-instance enforcement (as of 2026-08-05)

Reported live on a real install: the network status indicator (see above)
was permanently stuck on "Can't reach your Freenet node" after the app had
been running a while, with `query_error: "comm channel between client/host
closed"`. Root-caused directly rather than guessed at, and it turned out to
be two separate real bugs stacking:

- **`FreenetBridge` never reconnected.** The bundled Freenet sidecar's own
  auto-update supervisor (see "Supervise the bundled Freenet sidecar" above)
  is *working as designed* - it correctly killed and respawned the node
  process when a newer version (0.2.120) was detected - but nothing rebuilt
  the delegate's own outstanding WebSocket to the node that had just been
  replaced. Every `FreenetBridge` method held one connection for its entire
  lifetime with no path back to a working state once that connection died,
  exactly the gap this file's own "Known gaps" note under the network
  status indicator had flagged as a followup, now confirmed to actually
  happen on a real running install rather than just a theoretical risk.
  - Also found while fixing this: a **structural bug** in every method's
    retry loop. `api.send(request).await.map_err(...)?` used `?` to bail out
    of the *entire method* on a send failure, completely skipping the
    `for attempt in 1..=MAX_ATTEMPTS` retry loop below it - a send on a
    dead connection fails immediately and identically every time, so this
    meant a dead connection's first attempt was also its last, with no
    retry ever actually happening for that failure mode.
  - Fixed by folding a `send` failure into the same retryable `outcome` as a
    `recv` failure (so it participates in the existing retry loop instead of
    early-returning), and adding `FreenetBridge::reconnect` - rebuilds the
    WebSocket to the node in place - called between retry attempts in
    `put_new`/`update_state`/`get_state`/`subscribe`, and once (matching
    `query_node_status`'s existing "no flakiness-retry-loop" design - a dead
    transport is a different failure than gateway flakiness) in
    `query_node_status`. `watcher.rs`'s dedicated subscription bridge
    already had an equivalent fix at a coarser level (it discards and fully
    rebuilds its own `FreenetBridge` on any failure, see its `'connection`
    loop) and needed no change.
  - **Verified live**: connected a scratch delegate to a scratch node,
    confirmed `state: "connected"`, killed that node, confirmed the delegate
    correctly reported `state: "unknown"` with the exact
    `"comm channel between client/host closed"` error from the live bug
    report, started a *fresh* node process on the same port (simulating the
    supervisor's respawn), and confirmed `query_node_status` self-healed to
    `state: "connected"` with a real peer count - then confirmed an actual
    content operation (`get_latest_feed`, a `get_state` call) also
    recovered, not just the diagnostics query.
- **No single-instance enforcement**, which the tray change (see "Real
  desktop notifications" above) turned into a real, reproducible failure
  mode rather than a theoretical one: closing the window now hides instead
  of quitting, so a user (or Windows, or a stray shortcut) launching
  `aetheria.exe` again while the first copy is still alive in the tray used
  to spawn a **second, fully independent** set of Freenet + delegate
  sidecars, both pointing at the same shared Freenet data directory. Found
  directly on the live install this bug was reported on: two `aetheria.exe`
  processes running, only one with real sidecar children, and the Freenet
  log showing repeated `"Failed to load contract store ... Database already
  open. Cannot acquire lock"` from the second one colliding with the first.
  `tauri-plugin-single-instance` (registered first, per its own requirement,
  in `app/src-tauri/src/main.rs`) now intercepts a second launch before
  `.setup()` ever runs and just focuses the existing window instead.
  **Verified live**: launched the real installed app twice in a row -
  confirmed only one `aetheria.exe`/`aetheria-delegate.exe`/`freenet.exe`
  triplet ever exists, and a real `get_network_status` query against the
  survivor reported `state: "connected"`.

All 16 non-network delegate unit tests pass, `tsc` is clean, and
`npm run build:desktop` succeeds with no warnings.

## Why Aetheria isn't a pure Freenet web-container app, and cross-platform builds (as of 2026-08-06)

Prompted by real feedback on r/Freenet: a commenter asked why Aetheria needs
an installer at all, when "standard" Freenet apps are supposed to be a WASM
contract serving a UI straight from the local node's browser proxy, and
separately pointed out the installer only targets Windows. Investigated
both, with one real finding worth keeping and one real fix shipped.

### The web-container path was tried for real, and it's a dead end for this app's architecture

Freenet's actual "standard path" (confirmed by reading the manual and by
studying `freenet/freenet-microblogging`, an official example app in the
same domain as Aetheria - social/publishing, with its own identity/signing
delegate) is: UI as static HTML/CSS/JS published as a **web-container
contract** (`fdev website init`/`publish`/`update`), served by the local
node's own HTTP proxy at `http://<node>/v1/contract/web/<key>/`, with
private-key custody handled by a **Freenet delegate** - not a native
process, but a WASM component loaded into the user's own node, reachable
only via the node's own message-passing API.

**Verified live** against this machine's real running node
(`aetheria-test` key, `fdev website init`/`publish`): built `app/`'s
existing Vite frontend (after adding `base: "./"` to `vite.config.ts`, so
its asset paths resolve when served from a nested contract path rather than
site root - harmless for the Tauri build too, which loads from its own
custom protocol) and published it for real, producing a working
`/v1/contract/web/<key>/` URL that the real node served with a 200 and the
correct `index.html`/JS/CSS bytes.

**Then found the actual blocker**, by reading the node's own served wrapper
script (`freenetBridge`, the shim that lets a sandboxed contract iframe use
`WebSocket` at all) rather than assuming from the manual: every `open`
request is checked against `LOCAL_API_ORIGIN` and refused unless
`protocol://host` matches **the node's own origin exactly** - literal
source comment: *"Security: only allow WebSocket connections to the local
API server itself."* This is a deliberate, heavily-defended sandbox
boundary (auth-token injection, fail-closed hosted mode, per-user token
namespacing, anti-SSRF) - not a bug, not version-specific.

The consequence: a web-container-hosted UI **cannot** open a socket to
`ws://127.0.0.1:47021` (this app's native delegate) no matter what OS it's
built for - only back to the node it was served from. Since Aetheria's
entire trust model (`delegate/`'s Argon2id/AES-GCM identity encryption,
Ed25519 signing, SQLite cache, `watcher.rs`'s live-subscription push) lives
in that separate native process, the only way to get a genuine
zero-install "paste a URL into any browser" experience would be porting all
of that into a real WASM Freenet delegate running inside the node itself -
a large rewrite (WASM delegates only have a documented secrets-storage +
messaging API, no confirmed general local DB or background timers - the
system tray, OS toast notifications, and single-instance enforcement this
file documents above would need to be dropped or rethought). Deliberately
not started - see "Known stub" below. This finding, not a guess, is the
honest answer to "why not a pure webapp": the sandbox forecloses a cheap
middle ground, not that nobody looked into it.

### What shipped instead: cross-platform native builds

The narrower complaint - "why Windows only" - had a real, cheap fix once
checked: nothing in `delegate/` or `app/src-tauri/` is actually
Windows-specific (no `cfg(windows)`, no Windows-only crates - grepped
directly), so the Windows-only installer was a distribution gap, not an
architectural one. `.github/workflows/build-desktop.yml` (new) builds the
same three sidecars (`aetheria`, `aetheria-delegate`, `freenet`) natively on
Windows, macOS, and Linux GitHub-hosted runners and uploads each platform's
installer as a workflow artifact. The bundled Freenet sidecar is built from
source via `cargo install freenet` on every platform (same package/version
`refresh-latest-feed.yml` already uses) rather than sourced from a
per-platform prebuilt download - sidesteps needing to find/trust a binary
for each OS. `app/scripts/copy-build-artifacts.mjs` was made
platform-aware (binary suffix by `process.platform`, installer glob widened
to `.dmg`/`.app`/`.deb`/`.AppImage`/`.rpm`, `.app` bundles copied
recursively since they're directories) - it previously hardcoded `.exe`
paths and would have silently produced nothing on macOS/Linux.

**Honesty note, same caveat as `refresh-latest-feed.yml`**: written and
reviewed carefully, but not observed running on GitHub's own infrastructure
from this environment (no `gh` CLI here - see the environment notes above).
The macOS and Linux legs have never run anywhere, not even manually, since
this dev machine is Windows-only - the Windows leg's steps were spot-checked
locally (`cargo build --release --bin aetheria-delegate` from `delegate/`
still builds clean; the Vite `base: "./"` change was rebuilt and its output
verified to reference `./assets/...` not `/assets/...`), but the full
`tauri build` pipeline on all three OSes has not been re-run end-to-end
after these changes. Check the Actions tab after this merges, particularly:
whether Tauri v2's documented Linux dependency list (webkit2gtk,
appindicator, rsvg - copied from Tauri's own docs, not independently
re-derived against this app's exact plugin set) is complete, and whether
`cargo install freenet` finishes within the job's time budget on a cold
macOS/Linux runner (it's a real node, not a small crate - slow enough that
`refresh-latest-feed.yml` caches this same install specifically). Unsigned
installers remain unsigned on every platform (no cert configured) -
Windows shows the already-documented SmartScreen warning; macOS Gatekeeper
will very likely refuse to open the unsigned `.app` outright without the
user right-clicking → Open, a real friction point with no Windows
equivalent, not yet addressed.

## Known stub / unimplemented areas

- Payments/subscriptions are not a stub - they were built, verified live,
  and then deliberately removed (see "Payments and subscriptions removed"
  above). Every post is public; there is no reader-side access gap to close.
- Proof-of-work spam mitigation (design doc §7) and the pinning daemon
  (§7, §8 Phase 4) are not started. The Latest feed's 1000-entry cap (see
  above) is the closest thing to spam mitigation any part of this app has.
- A real WASM Freenet delegate (porting `delegate/`'s identity/signing/key-
  storage logic to run inside the user's own node, so the UI could be a
  pure web-container app with zero install) is not started - see "Why
  Aetheria isn't a pure Freenet web-container app" above for why this is a
  large rewrite, not a quick follow-up, and what native-only features
  (tray, OS notifications, single-instance enforcement) it would put at
  risk.
