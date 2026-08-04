import { useEffect, useRef, useState } from "react";
import { GlobeIcon } from "./icons";
import { delegate, type NetworkStatus } from "../lib/delegate";

/// Always-visible, live indicator of whether this machine's Freenet node is
/// actually part of the network - the one thing the app previously had no way
/// at all to tell the user.
///
/// The failure this exists for: a local node can be running, its API
/// reachable, and the delegate happily connected to it, while the node holds
/// **zero** peer connections. Every feed then renders empty, which is
/// pixel-identical to "the network genuinely has nothing new". Real causes
/// seen on this project directly: a leftover process squatting the node's
/// port, a bundled node too old to join the current network, and - most
/// commonly - a VPN routing P2P traffic through a tunnel that breaks NAT
/// hole-punching. All three looked exactly like an empty app.
///
/// Polls `get_network_status` (see `delegate/src/ipc.rs`), which asks the
/// node itself for its real peer count rather than inferring one.
const POLL_INTERVAL_MS = 5000;

/// What each state should say and look like. Kept as data rather than nested
/// ternaries in the JSX so every state is visible side by side and none can
/// silently fall through to a default that overstates connectivity.
function describe(
  status: NetworkStatus | null,
  unreachable: boolean,
): { dot: string; label: string; detail: string | null } {
  if (unreachable) {
    return {
      dot: "bg-red-500",
      label: "Delegate unreachable",
      detail:
        "The local Aetheria delegate isn't answering, so its Freenet connection can't be checked.",
    };
  }
  if (!status) {
    return { dot: "bg-neutral-600 animate-pulse", label: "Checking…", detail: null };
  }
  switch (status.state) {
    case "connected":
      return {
        dot: "bg-emerald-500",
        label:
          status.peer_count === 1
            ? "Connected — 1 peer"
            : `Connected — ${status.peer_count} peers`,
        detail: null,
      };
    case "isolated":
      return {
        dot: "bg-amber-500",
        label: "No peer connections",
        detail:
          "Your Freenet node is running but isn't connected to anyone, so feeds will look empty and posts won't publish. A VPN or a restrictive firewall is the most common cause — both can block the NAT hole-punching Freenet needs. It can also just take a minute or two on a cold start.",
      };
    case "unknown":
      return {
        dot: "bg-red-500",
        label: "Can't reach your Freenet node",
        detail:
          status.query_error ??
          "The local Freenet node didn't answer a status query.",
      };
    case "locked":
      return {
        dot: "bg-neutral-600",
        label: "Waiting for unlock",
        detail: "Freenet connects once your identity is unlocked.",
      };
  }
}

export default function NetworkStatusPanel() {
  const [status, setStatus] = useState<NetworkStatus | null>(null);
  const [unreachable, setUnreachable] = useState(false);
  // Guards against piling up requests if one poll is slow: the delegate
  // serializes every IPC request behind a single lock, so a status poll can
  // legitimately queue behind a long feed fetch. Skipping a tick is correct
  // here - the next one reports current truth anyway.
  const inFlight = useRef(false);

  useEffect(() => {
    let cancelled = false;

    async function poll() {
      if (inFlight.current) return;
      inFlight.current = true;
      try {
        const next = await delegate.getNetworkStatus();
        if (cancelled) return;
        setStatus(next);
        setUnreachable(false);
      } catch {
        if (cancelled) return;
        // Deliberately does not clear `status`: a delegate that briefly
        // stops answering shouldn't flash the panel back to "Checking…".
        setUnreachable(true);
      } finally {
        inFlight.current = false;
      }
    }

    void poll();
    const handle = setInterval(poll, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(handle);
    };
  }, []);

  const { dot, label, detail } = describe(status, unreachable);

  // A node with real peers whose operations are still failing is a genuinely
  // different problem from a node with no peers (it's the documented
  // gateway-network flakiness), so it gets its own line rather than being
  // folded into the headline state.
  const opsWarning =
    !unreachable && status?.state === "connected" && status.last_error
      ? "Recent network operations are failing — the public gateways can be flaky; retrying usually works."
      : null;

  return (
    <div className="rounded-xl border border-ink-800 bg-ink-900 p-5 text-sm text-neutral-400 leading-relaxed">
      <h3 className="text-neutral-100 font-semibold mb-2 flex items-center gap-2">
        <GlobeIcon className="w-4 h-4 text-neutral-500" />
        Freenet Network
      </h3>
      <p className="flex items-center gap-2 text-neutral-300">
        <span className={`w-2 h-2 rounded-full shrink-0 ${dot}`} aria-hidden="true" />
        <span>{label}</span>
      </p>
      {detail && <p className="mt-2 text-neutral-500">{detail}</p>}
      {opsWarning && <p className="mt-2 text-amber-500/90">{opsWarning}</p>}
    </div>
  );
}
