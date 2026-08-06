# Aetheria

Sovereign, censorship-resistant decentralized publishing platform — a free,
serverless replacement for Substack/Medium built on the [Freenet](https://freenet.org)
P2P network. Publish freely, follow other publishers; no accounts, no
payments, no platform that can pull your writing down.

Full design spec: [`docs/Decentralized_Substack_Design_Doc.pdf`](docs/Decentralized_Substack_Design_Doc.pdf)
(note: the design doc's payments/subscriptions sections no longer apply -
see `CLAUDE.md`'s "Payments and subscriptions removed" entry).

## Architecture

Three layers, isolating UI, key management, and network state:

| Layer | Location | Responsibility |
|---|---|---|
| 1. UI | [`app/`](app) | React 18 + Tauri desktop shell: markdown editor, reader feed. |
| 2. Local Delegate | [`delegate/`](delegate) | Rust daemon: key management, local SQLite cache, Freenet bridge. |
| 3. Freenet Contracts | [`contracts/`](contracts) | Rust WASM state contracts: `PublisherProfileContract`, `ContentIndexContract`, `PostDataContract`, `GlobalDirectoryContract`. |

The UI never touches key material or Freenet directly — it talks to the
Delegate over a loopback WebSocket (`delegate/src/ipc.rs`), and the Delegate
is the only thing that talks to the local Freenet node.

## Repository layout

```
contracts/                  Cargo workspace, compiles to wasm32-unknown-unknown
  aetheria-types/            Shared structs
  publisher-profile-contract/
  content-index-contract/
  post-data-contract/
  global-directory-contract/
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

Note: this section predates a lot of the project's actual progress - see
`CLAUDE.md` for the real, current state. Payments/subscriptions (originally
planned as "Phase 3" below) were built, verified live, and then removed
entirely - Aetheria is a free publish/follow platform now, not a roadmap
item.

1. **Phase 1 — WASM State Contracts**
2. **Phase 2 — Local Delegate & Local Publish/Read Loop**
4. **Phase 4 — Frontend UI, Pinning Daemon & Launch** (pinning daemon still
   not started - see `CLAUDE.md`'s "Known stub" section)
