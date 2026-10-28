# Contributing to StokesForge

Active development happens on `main`. Read
[`AGENTS.md`](AGENTS.md), [`.codex/README.md`](.codex/README.md) and the
documentation of the domain before proposing a change. Bugs and
changes are tracked in the GitLab issues. Commit messages, documentation
(`docs/`, except `docs/archive/`, which stays in French) and new code and
comments are in English; French may appear only in exempt places or behind a
`lang: fr` marker (test data, for instance: see "French guard" in
[`docs/tests.md`](docs/tests.md), run `python3 scripts/french-guard.py`). Existing French comments and identifiers are
translated crate by crate (#372–#376).

## Working with others

A contribution goes through a merge request to `main`: one review, green CI
required. No force-push and no rebase on a shared branch; the branch is
named `feature/<n>-<topic>`, stays short and receives `main` before the merge
(details: [`docs/v1-criteria.md`](docs/v1-criteria.md), "Branches and versions").

## License of contributions

Every original contribution to this branch must be offered under
**GPL-3.0-or-later**. Offer only material for which you hold the necessary
rights; keep the notices of third-party components and state where they
come from. Contributions to a vendored third-party copy keep its license,
and their changes must be identified.

Every external commit must carry a `Signed-off-by` line attesting the
[`Developer Certificate of Origin 1.1`](DCO), provided without modification.
After reading the DCO, `git commit -s` adds this line with your Git identity:

```text
Signed-off-by: First Last <address@example.org>
```

The sign-off confirms your right to offer the contribution under its
license; it does not transfer your copyright. No CLA is in place yet. The
project wants to be able to grant an additional permission later (for example
for a proprietary connector): a contribution accepted before a license CLA
(the contributor keeps their copyright and grants the right to redistribute
under other terms) would prevent this without the agreement of its author.
The CLA is tracked in #348 and must exist before the first external
contribution is merged.

## Checks

Follow the checks of the Codex guide: `fmt`, `clippy`, workspace tests and
Python tests when the Python path is changed. A new dependency or
resource must also respect [`deny.toml`](deny.toml) and appear
in the distribution notices. The license tools and commands are
in [`docs/license-and-extensions.md`](docs/license-and-extensions.md).

The Linux and Windows CI runs on GitHub (mirror of the repository); the
results are read in the GitLab pipeline of the commit or with
`scripts/ci-status.sh`. Instructions, runner pitfalls and maintenance:
[`docs/tests.md`](docs/tests.md), "Using CI day to day".

Any new extension interface covered by the exception requires an explicit
commit in the closed list of that same page, with its public contract. This
contribution agreement applies to `main`; it does not
change the license of the old implementation kept under the `legacy-python` tag.
