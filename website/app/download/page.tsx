import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Download | Aetheria",
  description: "Download Aetheria for Windows, macOS, or Linux.",
};

const VERSION = "0.1.0";

export default function Download() {
  return (
    <div className="max-w-3xl mx-auto px-6 py-16">
      <h1 className="text-3xl font-bold text-neutral-50">Download Aetheria</h1>
      <p className="mt-3 text-neutral-400">
        Version {VERSION} · Windows, macOS, and Linux, all 64-bit
      </p>

      <div className="mt-10 space-y-10">
        <div>
          <h2 className="text-lg font-semibold text-neutral-100">Windows</h2>
          <div className="mt-4 grid sm:grid-cols-2 gap-6">
            <div className="rounded-xl border border-ink-700 bg-ink-900 p-6 flex flex-col">
              <h3 className="text-base font-semibold text-neutral-100">Full Setup</h3>
              <p className="text-sm text-neutral-400 mt-2 flex-1 leading-relaxed">
                The one most people want. Installs Aetheria and a bundled
                Freenet node together, nothing else to set up first.
              </p>
              <a
                href="/downloads/Aetheria-Setup-x64.exe"
                className="mt-5 rounded-lg aetheria-gradient text-white text-sm font-semibold px-5 py-2.5 text-center shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition"
              >
                Download Aetheria-Setup-x64.exe
              </a>
              <p className="text-xs text-neutral-600 mt-2">≈ 16 MB</p>
            </div>

            <div className="rounded-xl border border-ink-700 bg-ink-900 p-6 flex flex-col">
              <h3 className="text-base font-semibold text-neutral-100">App Only</h3>
              <p className="text-sm text-neutral-400 mt-2 flex-1 leading-relaxed">
                For people who already have their own Freenet node running.
                Just the Aetheria app and its local delegate, no bundled node,
                no installer, unzip and run.
              </p>
              <a
                href="/downloads/Aetheria-app-only-x64.zip"
                className="mt-5 rounded-lg border border-ink-700 text-neutral-200 text-sm font-semibold px-5 py-2.5 text-center hover:bg-ink-800 transition"
              >
                Download Aetheria-app-only-x64.zip
              </a>
              <p className="text-xs text-neutral-600 mt-2">≈ 7 MB</p>
            </div>
          </div>
        </div>

        <div>
          <h2 className="text-lg font-semibold text-neutral-100">macOS</h2>
          <p className="mt-1 text-xs text-neutral-600">
            Apple Silicon (M1 and later) only - no Intel Mac build yet.
          </p>
          <div className="mt-4 grid sm:grid-cols-2 gap-6">
            <div className="rounded-xl border border-ink-700 bg-ink-900 p-6 flex flex-col">
              <h3 className="text-base font-semibold text-neutral-100">Full Setup</h3>
              <p className="text-sm text-neutral-400 mt-2 flex-1 leading-relaxed">
                Installs Aetheria and a bundled Freenet node together, same
                as the Windows Full Setup.
              </p>
              <a
                href="/downloads/Aetheria-Setup-macos-arm64.dmg"
                className="mt-5 rounded-lg aetheria-gradient text-white text-sm font-semibold px-5 py-2.5 text-center shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition"
              >
                Download Aetheria-Setup-macos-arm64.dmg
              </a>
              <p className="text-xs text-neutral-600 mt-2">≈ 25 MB</p>
            </div>
          </div>
        </div>

        <div>
          <h2 className="text-lg font-semibold text-neutral-100">Linux</h2>
          <p className="mt-1 text-xs text-neutral-600">
            64-bit (x86_64). AppImage runs on most distros with no
            installation; .deb and .rpm install through your package
            manager.
          </p>
          <div className="mt-4 grid sm:grid-cols-3 gap-4">
            <div className="rounded-xl border border-ink-700 bg-ink-900 p-5 flex flex-col">
              <h3 className="text-sm font-semibold text-neutral-100">AppImage</h3>
              <p className="text-xs text-neutral-500 mt-1 flex-1">Recommended - works on most distros</p>
              <a
                href="/downloads/Aetheria-x86_64.AppImage"
                className="mt-4 rounded-lg aetheria-gradient text-white text-xs font-semibold px-4 py-2 text-center shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition"
              >
                Download .AppImage
              </a>
              <p className="text-xs text-neutral-600 mt-2">≈ 100 MB</p>
            </div>

            <div className="rounded-xl border border-ink-700 bg-ink-900 p-5 flex flex-col">
              <h3 className="text-sm font-semibold text-neutral-100">.deb</h3>
              <p className="text-xs text-neutral-500 mt-1 flex-1">Debian, Ubuntu, and derivatives</p>
              <a
                href="/downloads/Aetheria-amd64.deb"
                className="mt-4 rounded-lg border border-ink-700 text-neutral-200 text-xs font-semibold px-4 py-2 text-center hover:bg-ink-800 transition"
              >
                Download .deb
              </a>
              <p className="text-xs text-neutral-600 mt-2">≈ 28 MB</p>
            </div>

            <div className="rounded-xl border border-ink-700 bg-ink-900 p-5 flex flex-col">
              <h3 className="text-sm font-semibold text-neutral-100">.rpm</h3>
              <p className="text-xs text-neutral-500 mt-1 flex-1">Fedora, RHEL, and derivatives</p>
              <a
                href="/downloads/Aetheria-x86_64.rpm"
                className="mt-4 rounded-lg border border-ink-700 text-neutral-200 text-xs font-semibold px-4 py-2 text-center hover:bg-ink-800 transition"
              >
                Download .rpm
              </a>
              <p className="text-xs text-neutral-600 mt-2">≈ 28 MB</p>
            </div>
          </div>
        </div>
      </div>

      <section className="mt-14 space-y-8">
        <div>
          <h2 className="text-lg font-semibold text-neutral-100">Installing</h2>

          <h3 className="mt-4 text-sm font-semibold text-neutral-200">Windows</h3>
          <ol className="mt-2 space-y-2 text-sm text-neutral-400 list-decimal list-inside">
            <li>Run the downloaded .exe.</li>
            <li>
              <strong className="text-neutral-300">
                Windows will probably show a &quot;Windows protected your
                PC&quot; warning.
              </strong>{" "}
              This is normal for a small open-source app without an
              expensive code-signing certificate. Click{" "}
              <strong className="text-neutral-300">More info</strong>, then{" "}
              <strong className="text-neutral-300">Run anyway</strong>.
            </li>
            <li>
              Windows Defender Firewall may ask to allow the bundled
              Freenet node to communicate. Allow it, or peer connectivity
              may be limited.
            </li>
          </ol>

          <h3 className="mt-5 text-sm font-semibold text-neutral-200">macOS</h3>
          <ol className="mt-2 space-y-2 text-sm text-neutral-400 list-decimal list-inside">
            <li>Open the .dmg and drag Aetheria into Applications.</li>
            <li>
              <strong className="text-neutral-300">
                Gatekeeper will refuse to open it the first time
              </strong>{" "}
              (&quot;Apple could not verify...&quot;) - same unsigned-app
              situation as Windows, no code-signing certificate yet.
              Right-click the app and choose{" "}
              <strong className="text-neutral-300">Open</strong>, then
              confirm in the dialog that appears. You only need to do this
              once.
            </li>
          </ol>

          <h3 className="mt-5 text-sm font-semibold text-neutral-200">Linux</h3>
          <ul className="mt-2 space-y-2 text-sm text-neutral-400 list-disc list-inside">
            <li>
              <strong className="text-neutral-300">AppImage:</strong> make
              it executable and run it -{" "}
              <code className="text-aecyan-400">chmod +x Aetheria-x86_64.AppImage</code>
              {" "}then double-click it, or run it from a terminal.
            </li>
            <li>
              <strong className="text-neutral-300">.deb:</strong>{" "}
              <code className="text-aecyan-400">sudo apt install ./Aetheria-amd64.deb</code>
            </li>
            <li>
              <strong className="text-neutral-300">.rpm:</strong>{" "}
              <code className="text-aecyan-400">sudo dnf install ./Aetheria-x86_64.rpm</code>
            </li>
          </ul>

          <p className="mt-5 text-sm text-neutral-400 leading-relaxed">
            On every platform, first launch creates a new,
            passphrase-protected identity.{" "}
            <strong className="text-neutral-300">
              Write your passphrase down somewhere safe.
            </strong>{" "}
            There is no recovery option if you lose it (see{" "}
            <a href="/docs/security" className="text-aeblue-400 hover:underline">
              Security &amp; your passphrase
            </a>
            ).
          </p>
        </div>

        <div>
          <h2 className="text-lg font-semibold text-neutral-100">
            Reinstalling or upgrading?
          </h2>
          <p className="mt-3 text-sm text-neutral-400 leading-relaxed">
            <strong className="text-neutral-300">Windows:</strong>{" "}
            uninstall the old version first (Settings → Apps, or run{" "}
            <code className="text-aecyan-400">uninstall.exe</code> from the
            install folder) before running a new installer over it.{" "}
            <strong className="text-neutral-300">macOS:</strong> drag the
            old Aetheria out of Applications to the Trash before dragging
            in the new one.{" "}
            <strong className="text-neutral-300">Linux:</strong>{" "}
            reinstalling the .deb/.rpm upgrades in place; for AppImage, just
            replace the old file with the new one. On every platform, your
            identity and posts live outside the install location, so
            they&apos;re unaffected by uninstalling or upgrading.
          </p>
        </div>
      </section>

      <section className="mt-14 space-y-8">
        <div>
          <h2 className="text-lg font-semibold text-neutral-100">
            System requirements
          </h2>
          <ul className="mt-3 space-y-1 text-sm text-neutral-400 list-disc list-inside">
            <li>Windows 10/11, macOS 12+ on Apple Silicon, or a modern 64-bit Linux distro</li>
            <li>
              Windows only: Microsoft Edge WebView2 Runtime (nearly always
              already present, Microsoft ships it via Windows Update); the
              installer will fetch it automatically if it&apos;s somehow
              missing, which needs internet access during install
            </li>
            <li>An internet connection, to reach the Freenet network</li>
          </ul>
        </div>

        <div>
          <h2 className="text-lg font-semibold text-neutral-100">
            Something else?
          </h2>
          <p className="mt-3 text-sm text-neutral-400 leading-relaxed">
            Intel Macs and ARM Linux aren&apos;t built yet. The source is
            open on{" "}
            <a
              href="https://github.com/HunterAug/Aetheria"
              className="text-aeblue-400 hover:underline"
            >
              GitHub
            </a>{" "}
            if you&apos;d like to build it yourself.
          </p>
        </div>
      </section>
    </div>
  );
}
