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

## Optional 2% platform fee (as of 2026-08-02)

Design doc §6.3's "Optional App Split": `handle_subscribe` in `ipc.rs`
requests a small fee invoice (2%, `PLATFORM_FEE_BASIS_POINTS = 200`) from a
second, separate `NwcClient` alongside the main subscription payment, paid
by the reader's already-connected wallet. Best-effort, non-blocking - a
hiccup collecting the fee never affects whether the subscriber gets
access (same philosophy as everything else in this file); the IPC response
carries `platform_fee_synced`/`platform_fee_error` so this is reported
honestly rather than silently swallowed either way.

- **Off by default.** `main.rs::connect_platform_fee_wallet` only connects
  this second wallet if `AETHERIA_PLATFORM_FEE_NWC` is set to a real
  `nostr+walletconnect://...` URI - unset for anyone else building/forking
  this project, so a fork doesn't silently try to pay a stranger's wallet.
- **Never commit the real connection string** to this repo, anywhere,
  including tauri sidecar env vars in `app/src-tauri/src/main.rs` (unlike
  `AETHERIA_DEV_PASSPHRASE`, which is an intentionally-public dev placeholder,
  this is a real secret). It's scoped receive-only when generated via
  `create-app --scopes "make_invoice,lookup_invoice,get_info,get_balance"`
  (no `pay_invoice`) specifically so a leak can't be used to spend funds -
  but "can't be drained" isn't the same as "safe to publish," so it stays
  out of version control regardless. How a real shipped installer gets this
  value into every user's copy (vs. a dev running it locally via env var)
  is an open question, not yet solved - flagged here rather than guessed at.
- The actual receiving wallet for this (as of 2026-08-02) is a self-hosted
  Alby Hub running in Docker (`ghcr.io/getalby/hub:latest`, container name
  `alby-hub`, port 8080, named volume `albyhub-data`) on **real Bitcoin
  mainnet** - not a testnet. It's initialized and unlocked (`setup`/`start
  --save` already run), but has **zero Lightning channels/liquidity** as of
  this writing (JIT channels are enabled though, so the first real payment
  it ever receives would open one automatically, fee deducted from that
  payment - see the hub's own `jit-channels.md` reference if the
  `getAlby/hub-skill` skill is installed).
- **Verified 2026-08-02, zero real money involved**: built and ran the
  actual `aetheria-delegate` binary with `AETHERIA_PLATFORM_FEE_NWC` pointed
  at a mock NIP-47 wallet (`delegate/examples/mock_nwc_wallet.rs`, real
  relay + real NIP-04 encryption, fake Lightning backend - same harness the
  NWC agent built for the main flow), drove a real `subscribe` IPC call:
  - Fee wallet ≠ reader wallet (two independent mock wallet processes): fee
    invoice creation succeeded, but paying it failed (`NotFound - unknown
    invoice`) - each mock wallet only recognizes invoices *it* created, an
    artifact of the two-independent-fake-ledgers test harness, not a real
    Lightning limitation (real bolt11 invoices are payable by any wallet
    that can route to them). Confirms the important thing: this failure did
    **not** block the main subscription - `network_synced: true`, a real
    preimage, full access granted; only `platform_fee_synced: false`.
  - Fee wallet = reader wallet (same mock wallet playing both roles, same
    "single identity plays multiple roles" convention already used for the
    main flow, see `nwc.rs`'s module docs): full success,
    `platform_fee_synced: true`, `platform_fee_error: null`, main
    subscription unaffected.

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
- **Reading a `SubscriberOnly` post from a publisher other than this
  delegate's own identity.** Decrypting it needs the ECDH shared secret,
  which needs that publisher's **secp256k1** identity public key
  (`identity_public_compressed()` - a completely different keypair from the
  Ed25519 `author_pubkey` this file's Following feature derives contract keys
  from). There is no mechanism yet for a reader to learn a stranger's
  secp256k1 pubkey - `subscriber_registry_e2e_test.rs`'s own module docs say
  so explicitly: in production it would arrive "via the peer-message channel
  design doc §5.2 step 2 describes", which isn't built. Deliberately not
  solved by this pass, and deliberately not worked around: `ipc.rs`'s
  `get_remote_post` refuses outright (checks the fetched payload's nonce
  independently, doesn't trust the caller) rather than attempting a decrypt
  that can only fail, and the UI renders these posts locked with a disabled
  open button instead of a broken "open" action. `contracts::fetch_key_bundle`
  is real, tested, and ready for the day this channel exists; nothing calls
  it from the Following path yet.
- Subscribing (paying via NWC) to a publication other than this delegate's
  own identity - Following only covers *reading*, not the NWC/ECDH-key-bundle
  flow; that flow's own docs above already note "no browsing UI exists yet"
  for the same underlying reason (no discovery mechanism), now narrowed
  specifically to the subscribe-and-pay path since read-only following is
  solved.
- Proof-of-work spam mitigation (design doc §7) and the pinning daemon
  (§7, §8 Phase 4) are not started.
