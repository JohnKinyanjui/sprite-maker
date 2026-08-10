# Contributing to Sprite Studio

Thank you for helping improve Sprite Studio. The project welcomes focused bug fixes, tests, documentation, platform support, and carefully scoped product improvements.

## Before opening a pull request

1. Search existing issues and pull requests to avoid duplicate work.
2. Keep the change focused on one clear problem or feature.
3. Explain the user impact and the reason for the implementation.
4. Add or update tests when behavior changes.
5. Include before-and-after screenshots or a short recording for visible UI changes.
6. Do not include unrelated formatting, generated files, or dependency changes.

For a large feature or architectural change, open an issue or discussion first. This avoids substantial work on a direction the project may not accept.

## Required checks

Run the relevant checks before requesting review:

```bash
bun install --frozen-lockfile
bun run check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Pull requests must report the relevant local quality checks in their validation notes. Changes are merged only after review by the project owner or an appointed maintainer. Review approval may be withdrawn when new commits materially change the reviewed patch.

## Review and acceptance

- The project owner has final authority over product direction, scope, and release readiness.
- A pull request may be declined even when it works if it conflicts with the product direction, duplicates another solution, increases maintenance cost disproportionately, or lacks sufficient validation.
- External pull requests require owner or maintainer approval and resolved review conversations before merge.
- Passing local or hosted automation is not approval by itself.
- AI-assisted contributions are allowed, but the author remains responsible for understanding, testing, and supporting the submitted code. Disclose substantial AI-generated work in the pull request.
- Do not submit copyrighted game assets, close copies of protected characters, secrets, private user data, or dependencies without a compatible license.

## Recognition policy

Git history and GitHub's contributor graph preserve authorship for accepted commits. Official project recognition and repository roles are separate, curated decisions.

A contributor becomes eligible for the README's **Recognized Contributors** list after either:

- 10 substantive pull requests have been merged; or
- the project owner explicitly approves early recognition.

A substantive pull request provides meaningful, reviewed value such as a tested bug fix, platform improvement, product feature, security fix, or significant documentation improvement. Trivial edits, automated churn, duplicate changes, abandoned work, spam, and changes that are later reverted do not count. The project owner decides whether a pull request is substantive and maintains the count.

Recognition does not grant merge, release, administration, or maintenance permissions. Maintainer status is invitation-only and may consider sustained quality, communication, judgment, security awareness, and project needs in addition to contribution count.

Project roles and recognition can be changed or removed by the project owner when needed for repository safety, accuracy, or governance. This policy does not rewrite valid Git authorship.

## Pull request checklist

- [ ] The pull request has one clear purpose.
- [ ] I tested the change locally.
- [ ] Frontend and native checks pass where applicable.
- [ ] I added tests or explained why tests are not applicable.
- [ ] I included visual evidence for UI changes.
- [ ] I disclosed substantial AI-generated code or assets.
- [ ] I included no secrets, private data, or incompatible assets.
- [ ] I read and agree to this contribution policy.
