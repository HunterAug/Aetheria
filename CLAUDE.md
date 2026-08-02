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

## Known stub / unimplemented areas

- `delegate/src/nwc.rs` — no real NWC/Nostr relay connection yet (Phase 3).
- `SubscriberRegistryContract` — untouched; no real NWC subscriber flow
  exists yet for it to serve (Phase 3), so it's still unwired on purpose.
- `FreenetBridge::subscribe` — sends nothing, still `todo!()`
  (`// TODO(Phase 4)`); nothing in the delegate consumes the
  `UpdateNotification` push responses a real subscription would trigger
  (pinning daemon / live feed updates, design doc §7-8), so wiring the send
  half up now would be a silent no-op.
- ECDH-based subscriber key delivery (`crypto::derive_shared_secret` and
  friends) is implemented but not called from anywhere yet — needs the NWC
  payment listener to trigger it (Phase 3).
- Per-post subscription tier is hardcoded to `required_tier_id: 0`
  (`ipc.rs`'s `handle_publish_post`) — the UI doesn't expose multiple tiers
  yet, and neither does the fresh `PublisherProfile` the delegate publishes
  on first run (`subscription_tiers: vec![]`, `title: "Untitled Publication"`).
- Proof-of-work spam mitigation (design doc §7) and the pinning daemon
  (§7, §8 Phase 4) are not started.
