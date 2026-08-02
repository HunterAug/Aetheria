export default function About() {
  return (
    <div className="px-6 py-5 max-w-2xl mx-auto">
      <h2 className="text-xl font-semibold text-neutral-100 mb-6">About</h2>

      <div className="space-y-5 text-[15px] leading-relaxed text-neutral-300">
        <p>
          Science relies on immutable, verifiable data. But right now, the
          global scientific record is hosted on centralized servers
          vulnerable to political interference, corporate interests, and
          silent scrubbing. When a government or institution decides a
          climate dataset, geological survey, or epidemiological study is
          politically inconvenient, it only takes one admin command to wipe
          it from the public record.
        </p>

        <p>We built this platform to fix that single point of failure.</p>

        <p>
          This is a sovereign, decentralized publishing protocol designed
          specifically to protect scientific truth. We don't rely on
          centralized cloud providers, we don't have host databases, and we
          do not have a kill switch.
        </p>

        <h3 className="pt-2 text-lg font-semibold text-neutral-100">
          The Architecture of Truth
        </h3>

        <p>
          Instead of living on a central server, the research and datasets
          published here are broken down, encrypted, and distributed
          globally across a peer-to-peer network using WebAssembly state
          contracts.
        </p>

        <p>
          Data is content-addressed and retrieved via cryptographic hashes.
          This means if a third party attempts to alter even a single comma
          in a published dataset, the cryptographic hash changes entirely,
          instantly proving the data was tampered with. It is mathematically
          impossible to silently alter or censor the work hosted on this
          network.
        </p>

        <h3 className="pt-2 text-lg font-semibold text-neutral-100">
          Our Mission
        </h3>

        <p>
          We are giving researchers, journalists, and scientists an
          unstoppable distribution channel. By stripping control away from
          centralized authorities, we ensure that empirical data and
          scientific fact remain permanently accessible to anyone, anywhere,
          regardless of local censorship laws or firewall restrictions.
        </p>

        <p className="pt-2 pb-2 text-lg font-semibold bg-aetheria-gradient bg-clip-text text-transparent">
          Scientific fact shouldn't have a server admin. Now, it doesn't.
        </p>
      </div>
    </div>
  );
}
