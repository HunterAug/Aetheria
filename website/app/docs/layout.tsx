import Link from "next/link";

const DOCS_NAV = [
  { href: "/docs", label: "Overview" },
  { href: "/docs/getting-started", label: "Getting started" },
  { href: "/docs/faq", label: "FAQ" },
  { href: "/docs/security", label: "Security & your passphrase" },
];

export default function DocsLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="max-w-5xl mx-auto px-6 py-16 flex gap-12">
      <nav className="w-48 shrink-0 hidden sm:block sticky top-16 self-start space-y-1">
        {DOCS_NAV.map((item) => (
          <Link
            key={item.href}
            href={item.href}
            className="block text-sm text-neutral-400 hover:text-neutral-100 py-1.5 transition-colors"
          >
            {item.label}
          </Link>
        ))}
      </nav>
      <div className="min-w-0 flex-1 max-w-2xl">{children}</div>
    </div>
  );
}
