# Contributing

Thanks for taking the time to contribute. ClashNova Desktop is a standalone sing-box proxy client; contributions that keep it simple, local, and dependency-light are the most welcome.

## Getting started

Set up the toolchain and run the app as described in the [Building](README.md#building) section of the README — Node.js with pnpm, Rust, Go, and Python 3.

For UI-only work you can run `pnpm dev` and preview in a browser; anything touching the connection, the core, or the TUN service has to be checked in the real desktop build.

## Before opening a pull request

- Type-check and build the frontend: `pnpm build`.
- Keep the Rust host compiling and its tests green: `cargo test` in `src-tauri/`.
- Add or update the unit tests under `tests/` when you change parsing, preferences, node ordering, or the connection state machine.
- If you change user-facing text, update every locale in `src/i18n/` so the languages stay in sync.
- Don't commit secrets, subscription URLs, or a `.env` file.

## Commit and PR style

- Write focused commits with a conventional-commit subject (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`), optionally scoped (`feat(nodes): ...`).
- Explain the "why" in the PR description, and mention anything you could not verify (for example, a platform you didn't build on).
- One logical change per PR is easier to review than a large mixed one.

## License

By contributing, you agree that your contributions are licensed under **GPL-3.0**, the same license as the project.

## Contact

Questions that aren't a bug report: clashnova.dev@gmail.com. For security issues, see [SECURITY.md](SECURITY.md).
