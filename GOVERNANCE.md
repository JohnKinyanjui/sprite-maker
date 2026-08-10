# Sprite Studio governance

Sprite Studio is an open-source project led by John Kinyanjui. Contribution credit and repository authority are intentionally separate: accepted work is credited immediately, while maintainer access is earned through sustained, trusted participation.

## Roles

### Contributor

Anyone whose issue, review, documentation, design, code, or other work improves Sprite Studio is a contributor. A merged commit may appear automatically in GitHub's contributor graph. Contributor status does not grant permission to push, merge, publish releases, manage secrets, or change repository settings.

### Maintainer

Maintainers may review or merge changes within the access explicitly granted to them. Maintainer access is never automatic and remains subject to repository protection rules.

To be considered, a contributor must normally have:

- at least 10 meaningful pull requests merged;
- at least 90 days of consistent, constructive participation;
- demonstrated care with testing, security, backwards compatibility, and review feedback;
- a record of respectful collaboration and sound technical judgment; and
- explicit approval from the project owner.

These are minimum eligibility signals, not an entitlement to access. Documentation-only count inflation, mechanical changes split across many pull requests, or low-quality submissions do not satisfy the intent of the policy.

Maintainers are expected to:

- protect the product direction and the safety of user data;
- review changes carefully and disclose conflicts of interest;
- keep `main` releasable;
- follow the security and release processes; and
- step back from privileged access when they are no longer active.

### Code owner

Code owners are trusted maintainers responsible for approving changes in a defined area. The repository-wide code owner is currently `@JohnKinyanjui`.

### Project owner

The project owner has final responsibility for product direction, maintainer appointments, security decisions, repository access, and releases.

## Decision making

Routine improvements are decided through pull-request review. Changes to product direction, security boundaries, supported platforms, release policy, or governance require project-owner approval.

When consensus is not possible, the project owner makes the final decision and documents material reasoning in the relevant issue or pull request.

## Repository access

- External contributors work through forks and pull requests.
- Merging one or more pull requests does not grant direct push access.
- Changes to protected branches require the configured reviews and automated checks.
- Repository and release credentials are limited to the minimum number of trusted maintainers.
- Access may be reduced or removed for inactivity, compromised credentials, policy violations, or project-safety concerns.

## Credit and releases

GitHub may automatically credit anyone whose authored commits reach the default branch. Sprite Studio does not rewrite valid contribution history merely to change that display.

Release notes are curated around user-facing changes. Individual acknowledgements may be included when they add useful context, but they are separate from maintainer status.

## Changing this policy

Governance changes are proposed through a pull request and require project-owner approval.
