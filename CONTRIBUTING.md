# Contributing to Aetheria

Thanks for considering it. This is a young, mostly solo-maintained project,
so please keep expectations calibrated: not every PR will be a fit, and
review may take a while.

## Before you start

Read the `CLAUDE.md` file closest to the code you're touching first — the
root [`CLAUDE.md`](CLAUDE.md) covers the overall architecture (delegate,
contracts, app), and `website/CLAUDE.md` covers the marketing site. These
are the actual up-to-date design notes for this project, more detailed and
more current than the [`README.md`](README.md).

For anything non-trivial (new features, protocol changes, anything
touching keys/crypto/payments), please open an issue first to discuss the
approach before writing code. It's a lot cheaper for everyone than a
finished PR that turns out to go a different direction than intended.

## Development setup

See the root [`README.md`](README.md#getting-started) for prerequisites and
the frontend/contracts/delegate dev commands. `scripts/build.sh`,
`scripts/dev-up.sh`, and `scripts/dev-down.sh` cover building and running
the local delegate + Freenet node stack together.

## Making changes

1. Fork the repo and create a branch off `main`.
2. Make your change. Keep PRs focused — one fix or feature per PR is much
   easier to review than several bundled together.
3. If you touched `delegate/` or `contracts/`, run `cargo check` (and
   `cargo test` where tests exist) before opening the PR.
4. If you touched `app/` or `website/`, run `npm run lint` in that
   directory.
5. Open a pull request against `main` with a clear description of what
   changed and why. Link any related issue.

Nothing merges without review — that's true for outside contributors and
for the maintainer alike.

## Reporting bugs / security issues

Open a GitHub issue for ordinary bugs. For anything you believe is a real
security vulnerability (especially around key handling or payments),
please don't open a public issue — use GitHub's private "Report a
vulnerability" flow instead (repo → Security tab → Advisories) so it can
be fixed before details are public.

## License

By contributing, you agree your contributions are licensed under this
project's [MIT license](LICENSE).
