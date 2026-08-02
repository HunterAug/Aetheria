# Aetheria

Sovereign, censorship-resistant decentralized publishing platform — a
serverless replacement for Substack/Medium built on the [Freenet](https://freenet.org)
P2P network, with non-custodial Lightning/NWC micropayments gating access to
encrypted articles.

Full design spec: [`docs/Decentralized_Substack_Design_Doc.pdf`](docs/Decentralized_Substack_Design_Doc.pdf).

## Architecture

Four layers, isolating UI, key/crypto management, network state, and
payments:

| Layer | Location | Responsibility |
|---|---|---|
| 1. UI | [`app/`](app) | React 18 + Tauri desktop shell: markdown editor, subscriber portal, reader feed. |
| 2. Local Delegate | [`delegate/`](delegate) | Rust daemon: key management, AES-256-GCM encryption pipeline, ECDH epoch-key exchange, NWC payment RPCs, local SQLite cache, Freenet bridge. |
| 3. Freenet Contracts | [`contracts/`](contracts) | Rust WASM state contracts: `PublisherProfileContract`, `ContentIndexContract`, `PostDataContract`, `SubscriberRegistryContract`. |
| 4. Settlement | Lightning / NWC (NIP-47) | Non-custodial peer-to-peer micro-settlement, driven from `delegate/src/nwc.rs`. |

The UI never touches key material or Freenet directly — it talks to the
Delegate over a loopback WebSocket (`delegate/src/ipc.rs`), and the Delegate
is the only thing that talks to the local Freenet node and to wallets.

## Repository layout

```
contracts/                  Cargo workspace, compiles to wasm32-unknown-unknown
  aetheria-types/            Shared structs (Tier, AccessTier, key bundles, ...)
  publisher-profile-contract/
  content-index-contract/
  post-data-contract/
  subscriber-registry-contract/
delegate/                   Native Rust daemon (Tokio), Layer 2
app/                         React + TypeScript + Tailwind, Tauri shell, Layer 1
  src-tauri/                  Tauri Rust shell around the web UI
docs/                        Design doc and reference material
```

## Prerequisites

This machine currently has Node.js and npm, but **no Rust toolchain and no
GitHub CLI installed yet**. Before working on `contracts/` or `delegate/`:

```bash
# Install Rust, then the wasm target the contracts compile to
rustup default stable
rustup target add wasm32-unknown-unknown
```

You'll also need a local Freenet node running to develop against Layer 3 —
see the [Freenet manual & SDK tutorial](https://freenet.org/build/manual/tutorial/).

## Getting started

```bash
# Frontend
cd app
npm install
npm run dev          # Vite dev server on :5173 (port 3000 is taken on this machine)
npm run tauri dev    # Full desktop shell, once the Rust toolchain is installed

# Contracts (once Rust + wasm32 target are installed)
cd contracts
cargo check

# Delegate daemon (once Rust is installed)
cd delegate
cargo run
```

## Status

Phase 1 (WASM contracts) and initial Phase 2/4 scaffolding are in place as
stubs — see inline `TODO`s in `delegate/src/nwc.rs` and
`delegate/src/freenet_bridge.rs` for what's unimplemented. Roadmap:

1. **Phase 1 — WASM State Contracts** (Weeks 1–4)
2. **Phase 2 — Local Delegate & Cryptographic Pipeline** (Weeks 5–8)
3. **Phase 3 — NWC Payment Engine & Auto-Key Delivery** (Weeks 9–11)
4. **Phase 4 — Frontend UI, Pinning Daemon & Launch** (Weeks 12–16)
