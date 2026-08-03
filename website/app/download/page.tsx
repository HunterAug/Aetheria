import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Download — Aetheria",
  description: "Download Aetheria for Windows.",
};

const VERSION = "0.1.0";

export default function Download() {
  return (
    <div className="max-w-3xl mx-auto px-6 py-16">
      <h1 className="text-3xl font-bold text-neutral-50">Download Aetheria</h1>
      <p className="mt-3 text-neutral-400">
        Version {VERSION} · Windows 10/11, 64-bit
      </p>

      <div className="mt-10 grid sm:grid-cols-2 gap-6">
        <div className="rounded-xl border border-ink-700 bg-ink-900 p-6 flex flex-col">
          <h2 className="text-lg font-semibold text-neutral-100">Full Setup</h2>
          <p className="text-sm text-neutral-400 mt-2 flex-1 leading-relaxed">
            The one most people want. Installs Aetheria and a bundled Freenet
            node together — nothing else to set up first.
          </p>
          <a
            href="/downloads/Aetheria-Setup-x64.exe"
            className="mt-5 rounded-lg aetheria-gradient text-white text-sm font-semibold px-5 py-2.5 text-center shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition"
          >
            Download Aetheria-Setup-x64.exe
          </a>
          <p className="text-xs text-neutral-600 mt-2">≈ 18 MB</p>
        </div>

        <div className="rounded-xl border border-ink-700 bg-ink-900 p-6 flex flex-col">
          <h2 className="text-lg font-semibold text-neutral-100">App Only</h2>
          <p className="text-sm text-neutral-400 mt-2 flex-1 leading-relaxed">
            For people who already have their own Freenet node running.
            Just the Aetheria app and its local delegate, no bundled node,
            no installer — unzip and run.
          </p>
          <a
            href="/downloads/Aetheria-app-only-x64.zip"
            className="mt-5 rounded-lg border border-ink-700 text-neutral-200 text-sm font-semibold px-5 py-2.5 text-center hover:bg-ink-800 transition"
          >
            Download Aetheria-app-only-x64.zip
          </a>
          <p className="text-xs text-neutral-600 mt-2">≈ 9 MB</p>
        </div>
      </div>

      <section className="mt-14 space-y-8">
        <div>
          <h2 className="text-lg font-semibold text-neutral-100">
            Installing (Full Setup)
          </h2>
          <ol className="mt-3 space-y-2 text-sm text-neutral-400 list-decimal list-inside">
            <li>Run the downloaded .exe.</li>
            <li>
              <strong className="text-neutral-300">
                Windows will probably show a &quot;Windows protected your PC&quot;
                warning.
              </strong>{" "}
              This is normal for a small open-source app without an
              expensive code-signing certificate — click{" "}
              <strong className="text-neutral-300">More info</strong>, then{" "}
              <strong className="text-neutral-300">Run anyway</strong>.
            </li>
            <li>
              Windows Defender Firewall may ask to allow the bundled Freenet
              node to communicate — allow it, or peer connectivity may be
              limited.
            </li>
            <li>
              First launch creates a new, passphrase-protected identity.{" "}
              <strong className="text-neutral-300">
                Write your passphrase down somewhere safe
              </strong>{" "}
              — there is no recovery option if you lose it (see{" "}
              <a href="/docs/security" className="text-aeblue-400 hover:underline">
                Security &amp; your passphrase
              </a>
              ).
            </li>
          </ol>
        </div>

        <div>
          <h2 className="text-lg font-semibold text-neutral-100">
            Reinstalling or upgrading?
          </h2>
          <p className="mt-3 text-sm text-neutral-400 leading-relaxed">
            Uninstall the old version first (Settings → Apps, or run{" "}
            <code className="text-aecyan-400">uninstall.exe</code> from the
            install folder) before running a new installer over it. Your
            identity and posts aren&apos;t stored in the install folder, so
            they aren&apos;t affected by uninstalling — only the app itself is
            removed.
          </p>
        </div>

        <div>
          <h2 className="text-lg font-semibold text-neutral-100">
            System requirements
          </h2>
          <ul className="mt-3 space-y-1 text-sm text-neutral-400 list-disc list-inside">
            <li>Windows 10 or 11, 64-bit</li>
            <li>
              Microsoft Edge WebView2 Runtime — nearly always already present
              on Windows 10/11 (Microsoft ships it via Windows Update); the
              installer will fetch it automatically if it&apos;s somehow
              missing, which needs internet access during install
            </li>
            <li>An internet connection, to reach the Freenet network</li>
          </ul>
        </div>

        <div>
          <h2 className="text-lg font-semibold text-neutral-100">
            Not on Windows?
          </h2>
          <p className="mt-3 text-sm text-neutral-400 leading-relaxed">
            Aetheria is currently Windows-only. The source is open on{" "}
            <a
              href="https://github.com/dakcalander-tech/Aetheria"
              className="text-aeblue-400 hover:underline"
            >
              GitHub
            </a>{" "}
            if you&apos;d like to build it for macOS or Linux yourself.
          </p>
        </div>
      </section>
    </div>
  );
}
