# Third-party binaries bundled with Aetheria

## Freenet (freenet-core)

Aetheria's desktop installer bundles an unmodified, official `freenet.exe`
build as a second Tauri sidecar (alongside `aetheria-delegate.exe`) so the
app works out of the box with no separate Freenet install.

- Project: https://github.com/freenet/freenet-core
- License: **GNU Affero General Public License v3.0 (AGPL-3.0)**, per the
  project's own `LICENSE.md`
  (https://raw.githubusercontent.com/freenet/freenet-core/main/LICENSE.md).
  `freenet-stdlib` (the crate Aetheria's own Rust code links against
  directly, in `contracts/` and `delegate/`) is separately licensed LGPL by
  the same project specifically so applications built against it - like
  Aetheria - can remain proprietary; only `freenet-core` itself (the node
  binary) is AGPL.
- Bundled version: **0.2.118** (commit `738f99f0e2e7`, built
  2026-08-02T16:44:29Z per `freenet.exe --version`).

### Why bundling this binary doesn't AGPL-license Aetheria

freenet-core's own licensing summary states: "applications merely
communicating with Freenet over standard protocols (HTTP, WebSocket)
without directly linking to freenet-core are not derivative works subject
to AGPL requirements." Aetheria's delegate talks to the bundled node only
over its loopback WebSocket API
(`ws://127.0.0.1:7509/v1/contract/command?...`, see
`delegate/src/freenet_bridge.rs`) - the same interface any independent
Freenet client uses - and never links against `freenet-core` itself. The
node binary and Aetheria's own binaries (`aetheria.exe`,
`aetheria-delegate.exe`) are shipped side by side in one installer purely
as an aggregation of independent programs, not a combined/derivative work.

### What AGPL-3.0 does require here

The bundled `freenet.exe` itself is conveyed unmodified, which AGPL-3.0
permits with copyright/warranty notices intact. To stay compliant:

- This file documents the exact upstream version/commit bundled, and links
  to its public source (the tag/commit above on
  https://github.com/freenet/freenet-core).
- The AGPL-3.0 license text applies to `freenet.exe` itself; see
  https://www.gnu.org/licenses/agpl-3.0.txt for the full text (not
  reproduced here to avoid duplicating a large legal document that can be
  fetched authoritatively from its source).
- Aetheria's own source (this repository) is under whatever license the
  repository root specifies for Aetheria - unaffected by AGPL, per the
  "not a derivative work" note above.

This is a good-faith compliance summary, not legal advice - if bundling
scope changes (e.g. patching `freenet-core` itself rather than shipping it
unmodified, or linking against it directly instead of over the network
API), re-review AGPL-3.0 §13's remote-network-interaction clause before
shipping.
