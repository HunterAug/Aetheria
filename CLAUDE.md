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
  `http://127.0.0.1:7509/`. Its WebSocket API is `ws://127.0.0.1:7509/`
  (root path, confirmed by reading `freenet-stdlib`'s `client_api` source
  and by successfully round-tripping a contract through it - see below).
  The node currently shows "only connected to gateways, NAT hole-punching
  0/N" — that's about *other peers reaching this node*, not about this
  node's own ability to GET/PUT contract state through its gateways, which
  works fine (proven both by an official demo app loading through it and by
  our own published test contract).
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

## Known stub / unimplemented areas

- `delegate/src/nwc.rs` — no real NWC/Nostr relay connection yet (Phase 3).
- `delegate/src/freenet_bridge.rs` — no real Freenet client API calls yet
  (still `todo!()`); nothing from the delegate is broadcast to the network,
  everything in "Working end-to-end" above is local-only. We proved the
  underlying network path works via the `fdev` CLI directly (see above) -
  the remaining work is wiring `FreenetBridge` to do the same thing
  programmatically via `freenet_stdlib::client_api::WebApi` instead of
  shelling out to `fdev`.
- ECDH-based subscriber key delivery (`crypto::derive_shared_secret` and
  friends) is implemented but not called from anywhere yet — needs the NWC
  payment listener to trigger it (Phase 3).
- Proof-of-work spam mitigation (design doc §7) and the pinning daemon
  (§7, §8 Phase 4) are not started.
