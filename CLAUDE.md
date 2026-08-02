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
    `post-data-contract/`, `subscriber-registry-contract/` — one crate per
    contract from design doc §3.
- `delegate/` — native Rust daemon (Tokio), Layer 2. Owns keys, crypto,
  Freenet bridge, NWC payments, local SQLite cache. Never expose key
  material or ciphertext across the IPC boundary to the UI — only decrypted
  content and derived state.
- `app/` — React 18 + TypeScript + Tailwind + Tauri, Layer 1. Talks to the
  delegate only via the loopback WebSocket in `app/src/lib/delegate.ts`.

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

## Conventions

- Contract state structs live in `aetheria-types` if used by more than one
  contract or by the delegate; contract-local structs stay in that crate.
- Epoch-key crypto math (ECDH → HKDF → AES-256-GCM) lives in
  `delegate/src/crypto.rs` and should mirror design doc §4.2 exactly —
  that section is the spec for interop between publisher and subscriber
  delegates, don't improvise a different KDF or nonce scheme.
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

TODO(later), noted in `keys.rs`'s module docs: once Tauri spawns this as a
sidecar with no attached terminal, this needs a real `unlock { passphrase }`
IPC message the UI sends before any signing operation, not a stdin prompt.

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
  `"externalBin": ["binaries/aetheria-delegate"]` in `tauri.conf.json` and
  `.sidecar("aetheria-delegate")` in Rust, the actual file must exist at
  `app/src-tauri/binaries/aetheria-delegate-x86_64-pc-windows-msvc.exe` on
  this machine. That file is **not source** - it's a straight copy of
  `delegate/target/{debug,release}/aetheria-delegate.exe`, gitignored
  (`app/src-tauri/binaries/` in the root `.gitignore`), and must be
  regenerated locally before `npm run tauri dev`/`build` after any delegate
  rebuild:
  ```
  cp delegate/target/debug/aetheria-delegate.exe \
     app/src-tauri/binaries/aetheria-delegate-x86_64-pc-windows-msvc.exe
  ```
  (swap `debug` for `release` for a production bundle).
- **Vite/Tauri interaction bug**: `tauri dev` builds the Rust shell inside
  `app/src-tauri/target/` while Vite's dev server is also running from
  `app/`. Vite's default file watcher picks up churn in that directory
  (including a build-script binary mid-write/locked by cargo), and on
  Windows that throws an `EBUSY` error that crashes Vite's process instead
  of just logging a warning - which in turn kills `tauri dev`'s
  `beforeDevCommand` step. Fixed in `app/vite.config.ts` with
  `server.watch.ignored: ["**/src-tauri/**"]`.
- **Passphrase stopgap (temporary, not a real solution)**: the sidecar is
  spawned with `AETHERIA_DEV_PASSPHRASE=aetheria-dev-local-only` and
  `RUST_LOG=info` set via `.env(...)` in `main.rs`, so `delegate/src/keys.rs`'s
  encrypted-identity unlock doesn't block forever on an `rpassword` prompt
  with no attached terminal to read from (see the "Identity key encryption"
  section above for why that prompt exists). This is marked with a
  `// TODO(next):` comment in `main.rs`. The real fix is an in-app "enter
  your passphrase to unlock" screen that sends a new `unlock { passphrase }`
  IPC request to the delegate before any signing operation - deliberately
  **not** implemented now because it needs changes to
  `delegate/src/ipc.rs` and `delegate/src/keys.rs`, both of which a
  concurrent session is actively editing for the NWC/Lightning payment
  feature; touching them here would conflict. Follow-up work, tracked as
  the main remaining gap in this area.
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

One caveat: the delegate's own `cargo build --release` was failing at build
time due to unrelated in-progress edits from the concurrent NWC-feature
session (`ipc.rs`/`db.rs` calling a `get_epoch_key` method that doesn't
exist yet on `LocalStore` - not something to fix from here, and not touched).
The sidecar binary bundled into this installer is therefore the **debug**
build of `aetheria-delegate.exe` copied into `app/src-tauri/binaries/`
before that breakage happened - functionally correct (it's the same binary
verified working end-to-end above) but not release-optimized. Re-copy a
real `cargo build --release` output into
`app/src-tauri/binaries/aetheria-delegate-x86_64-pc-windows-msvc.exe` and
rebuild once the delegate compiles clean again, before treating this as a
real release artifact.

## NWC subscription flow: real ECDH key delivery + real NIP-47 (as of 2026-08-02)

Phase 3 (design doc §5.2/6.1, Workflow B) is implemented: a reader connects a
Lightning wallet via Nostr Wallet Connect, subscribes to a tier, and gets an
ECDH-encrypted epoch key bundle appended to a real `SubscriberRegistryContract`
on the real Freenet network. Three things were built and are worth
distinguishing by how thoroughly each was actually verified:

**(a) Verified for real, with concrete evidence:**

- **ECDH key delivery + `SubscriberRegistryContract` over the real network.**
  `delegate/src/contracts.rs` gained `subscriber_registry_key_for` (a *pure,
  local* computation - `ContractKey::from_params_and_code(params, code)` is
  a deterministic hash, the same one `FreenetBridge::put_new` computes
  internally, so any delegate holding the same compiled contract code and a
  publisher's Ed25519 pubkey can independently derive their
  `SubscriberRegistryContract` key with **no discovery call, no pointer
  field anywhere** - this is why the contract doesn't need a "where do I
  find this" field), plus `ensure_subscriber_registry` (mint-once, lazy -
  only the first time someone actually subscribes, unlike
  `ensure_publisher_identity`'s eager content_index/profile),
  `publish_key_bundle_to_network` (publisher side), and `fetch_key_bundle`
  (reader side, network-only, no local DB dependency).
  `delegate/src/subscriber_registry_e2e_test.rs` (`#[cfg(test)]`, declared
  from `main.rs`, `#[ignore]`d since it needs a live node - run with `cargo
  test subscriber_registry_e2e -- --ignored --nocapture`) simulates two
  *genuinely independent* secp256k1 identities (not the single-identity
  degenerate case the IPC handler exercises) round-tripping a real epoch key
  through the real network: publisher encrypts, publishes; a second,
  independent `FreenetBridge` connection (standing in for a different
  process) fetches and decrypts using only its own secret + the publisher's
  known public keys, and recovers the exact same epoch key. Independently
  re-confirmed both this test's contract and the full IPC `subscribe` flow's
  contract with `fdev -p 7509 execute get <key>` from a separate shell (same
  methodology as the post-data/content-index verification above) - real CBOR
  bytes, real `bundles` array, matching `EncryptedKeyBundle`'s schema.
- **Real NIP-47 protocol mechanics, over a real public relay, zero real
  money.** `delegate/src/nwc.rs` uses the `nwc` crate (rust-nostr project,
  pinned to the stable `0.44.0` - `0.45.x` is alpha-only as of 2026-08, see
  that file's module docs for the version survey and for working out the
  actual request direction from the real NIP-47 spec instead of the design
  doc's misleading §6.1 wire sketch). `nwc` only implements the *client*
  side of NIP-47 (talks to a wallet you already have), so there was no
  ready-made way to test it without a funded real wallet - solved the same
  way this project solved "no external infra to test against" for Freenet
  (`fdev` + a local node): built a mock NIP-47 *wallet service* directly on
  `nostr-sdk` (dev-dependency only, not shipped) in
  `delegate/examples/nwc_protocol_check.rs` - real Nostr keys, a real
  connection to `wss://nos.lol` (public relay; `wss://relay.damus.io`
  intermittently 503s, use `AETHERIA_NWC_TEST_RELAY` to override), real
  kind 13194/23194/23195 events, real NIP-04 encryption. It drives two
  independent `nwc::NWC` client connections (different per-app secrets, same
  mock wallet pubkey - exactly how one real wallet serves multiple apps)
  through `make_invoice` → `pay_invoice` → `lookup_invoice`, and passed:
  `cargo run --example nwc_protocol_check` prints the full encrypted
  request/response trace and a final PASS. `delegate/examples/mock_nwc_wallet.rs`
  is the same mock wallet as a long-running process (prints its connection
  URI) - used to drive the **actual production code**, not just the
  protocol-check harness: ran the real `aetheria-delegate` binary
  (`AETHERIA_DEV_PASSPHRASE` set, this machine's real existing identity) and
  called `connect_wallet` → `get_subscription_info` → `subscribe` →
  `list_subscribers` over the real IPC socket (a small Python
  `websockets` script, `cargo run` doesn't apply here). Real
  `SubscriberRegistryContract` key `DuxRTe51t6WTwpnFiHGDeX1egRQGC1ZA7shs8TgxPwEM`
  was independently confirmed via `fdev execute get` afterward.
- **`SubscriberPortal.tsx` end-to-end in the browser.** Real "Connect
  Wallet" input (paste a `nostr+walletconnect://...` URI), real tier
  display, real "Subscribe" action - all driven through
  `app/src/lib/delegate.ts`'s new `connectWallet`/`getSubscriptionInfo`/
  `subscribe`/`listSubscribers` methods, no placeholders. Verified rendering
  real data (the real publication key, the hardcoded tier, and the real
  subscriber entry from the `list_subscribers` test above) via the Vite dev
  server against the real running delegate.

**(b) Built correctly per spec, not fully verifiable without real funds:**

- A real end-to-end payment against a **real funded Lightning wallet** was
  never attempted, per this task's explicit scope (no real money/sats, no
  signing up for any funded service). `NwcClient::pay_invoice` is exercised
  above only against the mock wallet's fake settlement - the NIP-47 wire
  mechanics are proven real, but real Lightning settlement itself (routing,
  fees, actual on-chain/off-chain finality) is the one piece that
  genuinely needs the user's own wallet to verify.

**(c) Left as TODO, matching this task's explicit scope:**

- `default_tiers()` in `ipc.rs` hardcodes a single "Supporter" tier
  (5,000 sats/month) - real multi-tier configuration in Settings
  (populating `PublisherProfile.subscription_tiers`, still always `vec![]`
  from `ensure_publisher_identity`) was explicitly out of scope.
- `handle_subscribe` verifies settlement via NIP-47 `lookup_invoice` polling
  rather than the optional real-time notification extension (kind 23196) -
  `lookup_invoice` is base-spec, universally supported; notifications are
  an add-on some wallets skip. See `nwc.rs`'s module docs.
- A reader subscribing to a publication that *isn't* their own identity has
  no UI yet - this app has no concept of browsing other publications at all
  (same limitation the existing single-identity publish/feed loop already
  has, see above). `contracts::fetch_key_bundle` is real, tested, and ready
  for that reader-side code path once it exists; it's just not called from
  `ipc.rs` yet.

## Known stub / unimplemented areas

- `FreenetBridge::subscribe` — sends nothing, still `todo!()`
  (`// TODO(Phase 4)`); nothing in the delegate consumes the
  `UpdateNotification` push responses a real subscription would trigger
  (pinning daemon / live feed updates, design doc §7-8), so wiring the send
  half up now would be a silent no-op.
- Per-post subscription tier is hardcoded to `required_tier_id: 0`
  (`ipc.rs`'s `handle_publish_post`) — the UI doesn't expose multiple tiers
  yet, and neither does the fresh `PublisherProfile` the delegate publishes
  on first run (`subscription_tiers: vec![]`, `title: "Untitled Publication"`).
- Real Lightning payment settlement against a funded wallet - see the NWC
  section above; everything up to and including the protocol/network layer
  is verified, real money movement is not.
- Browsing/subscribing to a publication other than this delegate's own
  identity - no discovery UI exists yet; see the NWC section above.
- Proof-of-work spam mitigation (design doc §7) and the pinning daemon
  (§7, §8 Phase 4) are not started.
