import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import Link from "next/link";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "Aetheria — Censorship-resistant publishing on Freenet",
  description:
    "Aetheria is a decentralized, serverless publishing platform. No company can take down your posts, suspend your account, or read your subscribers' payment details.",
};

const NAV = [
  { href: "/", label: "Home" },
  { href: "/download", label: "Download" },
  { href: "/docs", label: "Docs" },
  { href: "/latest", label: "Latest posts" },
];

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`${geistSans.variable} ${geistMono.variable} h-full antialiased`}
    >
      <body className="min-h-full flex flex-col bg-ink-950 text-neutral-200">
        <header className="border-b border-ink-800">
          <div className="max-w-5xl mx-auto px-6 py-4 flex items-center justify-between">
            <Link href="/" className="flex items-center gap-2">
              <img src="/logo.png" alt="" className="w-7 h-7" />
              <span className="text-lg font-semibold tracking-tight text-neutral-100">
                Aetheria
              </span>
            </Link>
            <nav className="flex items-center gap-6 text-sm">
              {NAV.slice(1).map((item) => (
                <Link
                  key={item.href}
                  href={item.href}
                  className="text-neutral-400 hover:text-neutral-100 transition-colors"
                >
                  {item.label}
                </Link>
              ))}
              <a
                href="https://github.com/dakcalander-tech/Aetheria"
                className="text-neutral-400 hover:text-neutral-100 transition-colors"
              >
                GitHub
              </a>
            </nav>
          </div>
        </header>

        <main className="flex-1">{children}</main>

        <footer className="border-t border-ink-800">
          <div className="max-w-5xl mx-auto px-6 py-8 flex flex-col sm:flex-row items-center justify-between gap-3 text-sm text-neutral-500">
            <p>
              Aetheria is free, open-source software. No company runs it, no
              company can shut it down.
            </p>
            <div className="flex items-center gap-5">
              <Link href="/docs" className="hover:text-neutral-300">
                Docs
              </Link>
              <Link href="/download" className="hover:text-neutral-300">
                Download
              </Link>
              <a
                href="https://github.com/dakcalander-tech/Aetheria"
                className="hover:text-neutral-300"
              >
                Source
              </a>
            </div>
          </div>
        </footer>
      </body>
    </html>
  );
}
