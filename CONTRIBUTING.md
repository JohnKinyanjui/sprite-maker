# Contributing to Sprite Studio

Thank you for helping improve Sprite Studio. Contributions are welcome through focused pull requests that are easy to understand, test, and review.

## Before starting

- Search existing issues and pull requests before opening a duplicate.
- Open an issue first for large features, architectural changes, new providers, or changes to generated file formats.
- Keep each pull request focused on one coherent outcome.
- Never include credentials, private workspace data, generated user assets, or third-party material without a compatible license.

## Development workflow

1. Fork the repository and create a descriptive branch.
2. Install dependencies with `bun install --frozen-lockfile`.
3. Make the smallest complete change that solves the problem.
4. Add or update tests where behavior changes.
5. Run the required checks:

   ```bash
   bun run check
   cargo test --manifest-path src-tauri/Cargo.toml
   cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
   ```

6. Open a pull request using the repository template and explain the user impact.

## Pull-request expectations

- Describe what changed and why.
- Include screenshots or short recordings for visible UI changes.
- Call out migrations, compatibility risks, or platform-specific behavior.
- Preserve unrelated code and user-authored changes.
- Respond to review feedback with new commits rather than rewriting shared history during review.
- Keep generated binaries and local workspace data out of the repository.

All changes remain subject to code-owner review and required automated checks. A merged pull request gives contribution credit but does not grant direct repository access. Maintainer eligibility is described in [GOVERNANCE.md](GOVERNANCE.md).

## Reporting security issues

Do not open a public issue for a vulnerability that could put users or their files at risk. Use GitHub's private vulnerability-reporting flow when it is available for the repository.

## License

By submitting a contribution, you agree that it may be distributed under the repository's MIT license.
