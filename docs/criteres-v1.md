# V1 criteria (#333)

V1 ships when **every box below is true**, as observed on the commit
that carries the `v1.0.0` tag. This page says what is required and how to
observe it; the content of the version is in [`CHANGELOG.md`](../CHANGELOG.md).

## What V1 promises

| Promise | Held by |
|---|---|
| A `.stf` file written by V1 can be read back and migrated up to V3; a file of a newer **format** (higher format number) is refused with a clear message | [`persistance.md`](persistance.md) § Version rule: every V1 file can be read back up to V3, [`invariants.md`](invariants.md) no. 12, witness file `crates/stokesforge-persistence/tests/temoins/v1/` |
| A `pyst` script written against V1 keeps working | [`python-et-script.md`](python-et-script.md) § Frozen API V1 (#331), test `crates/stokesforge-script/tests/surface_figee.rs` |
| The application and its embedded Python install and start on Linux x86_64 and Windows x86_64 | `scripts/app-dist.sh`, `scripts/app-dist-windows.sh` ([`architecture.md`](architecture.md), § Where the binaries are) |

The rest of the code has **no** compatibility promise from one major
version to the next (see "Branches and versions" below).

## Criteria

Each line is observed by a command or a page; none is declared
"done" without having been observed.

### Content

- [x] All issues of the V1 milestone are closed (`glab issue list --milestone V1`
  returns nothing).
- [x] No open issue labeled `bug` touches data loss, the
  `.stf` format or the Python bridge (`glab issue list --label bug`).

### Quality

- [x] On the tag commit, the blocking jobs are green: `fmt`, `licenses`,
  `clippy`, `test`, `test-python` (`scripts/ci-statuts.sh <commit>`).
- [x] `test-python-windows` and `dist-windows` (automatic on `main` and on a tag) are green on the same commit.
- [x] `cargo test -p stokesforge-robustesse -- --ignored` returns no failure
  (zero red: no `#[ignore]` is left, see `docs/tests.md`).

### Distribution

- [x] `scripts/app-dist.sh` exits 0 on Linux: the window opens and closes,
  `stokesforge-selftest` passes, from the package. CI does this under a virtual
  display (xvfb) at the tag; before the tag, also try it by hand on a real
  Wayland desktop. Manual trial done on 2026-10-26 on Ubuntu 26.04 (GNOME,
  Wayland, VM, software rendering) rather than on Fedora, to get away from the
  development distribution: package built on the dev machine,
  copied out of the repository, `stokesforge-selftest` code 0, application, `pyst`/numpy/scipy
  console, import, `.stf`, exports and drag and drop
  work. Left to tick the box: CI at the tag, and repeat the same trial
  with the archive of the latest `-rcN` (`v1.0.0-rc6`: built on Ubuntu by
  CI, in the GitLab package registry). Also observe the first
  opening there: the demo opens on first launch, the next one starts
  on an empty project (to replay: delete `~/.config/StokesForge`).
- [x] `scripts/app-dist-windows.sh` exits 0 on the Windows runner and the installer
  `StokesForge-<version>-windows-setup.exe` installs, starts and uninstalls
  on a fresh machine: at least once by hand, on another Windows
  computer, with the installer of the latest `-rcN` (`v1.0.0-rc6`, same registry; the runner
  has no desktop), first opening included (demo, then empty project).
- [x] The license notices are shipped in both packages, and those for
  Windows are cross-checked against the complete python-build-standalone Windows archive
  (`vendor/python/licenses-windows/README.md`).
- [x] Windows shows "unknown publisher" (no code signing): accepted
  (V1 decision) and written in the release notes, section « Installation sous Windows »
  of `CHANGELOG.md`.

### Version and documentation

- [x] All crates are at `1.0.0` (`scripts/versions-alignees.sh` checks it on every push, `scripts/release-notes.sh v1.0.0` at the tag).
- [x] `CHANGELOG.md` has a dated `## [1.0.0]` section, without « à dater ».
- [x] The pages of `docs/` cited by the promises above describe the code
  as it is (review, no reference to an issue that is still open).

## Publishing

1. Observe the criteria above; date the `## [1.0.0]` section of the CHANGELOG;
   the token that lets GitHub upload the packages to GitLab and create the
   release there already exists (GitLab project token, Developer role, `api` scope,
   GitHub secret `GITLAB_RELEASE_TOKEN`); it **expires**: check its expiry date
   before the tag (`docs/tests.md`, "Maintenance").
2. Merge into `main`: there is no version branch to create.
3. Put the `v1.0.0` tag on this commit of `main` and push it to GitLab (the
   mirror passes it on to GitHub).
4. The tag triggers, on GitHub only (the mirror passes it on):
   - the blocking checks, Linux and Windows;
   - `release-linux` and `dist-windows`: archive
     `StokesForge-1.0.0-linux-x86_64.tar.gz` and installer
     `StokesForge-1.0.0-windows-setup.exe`, uploaded to the GitLab package
     registry (no GitHub release: GitHub is only a mirror);
   - `publier`, when all of that is done: GitLab release (notes taken from the
     CHANGELOG, links to the two packages). No GitLab runner is used for this.
5. Observe: the GitLab release exists, the installer and the archive can be
   downloaded from it.

**Variant for a following version `X.Y.Z`** (`1.0.1`, `1.1.0`, `2.0.0`): the
steps are the same, with what changes:

1. A dedicated version commit: `version = "X.Y.Z"` in all crates,
   `cargo update --workspace`, section `## [X.Y.Z] - date` of the CHANGELOG (the
   `[Non publié]` section is renamed and dated there).
2. Green CI on this commit (`scripts/ci-statuts.sh <commit>`), `test-python-windows`
   and `dist-windows` included.
3. Tag `vX.Y.Z` on this commit of `main` (or of `release/N.x` for a fix
   of an older major version), **on the explicit order of the user**. An
   `-rcN` first only in the cases of "Branches and versions".

The release jobs refuse an inconsistent tag before any upload:
form `vX.Y.Z`, crate versions, CHANGELOG section (`scripts/release-notes.sh`).
The alignment of crate versions is already checked on every push by `fmt`
(`scripts/versions-alignees.sh`: crates, internal dependencies, `Cargo.lock` —
a forgotten `cargo update --workspace` fails —, format witness): the tag
adds the form, the CHANGELOG, the refusal of a tag outside `main`/`release/*`
(`scripts/release-tag-sur-main.sh`) and of a tag that does not exceed the
existing tags (`scripts/release-tag-croissant.sh`).

**What has been proven, and what has not.** The tags `v1.0.0-rc1` to
`-rc6` served as trials and remain as a trace (`rc1` red, fixed by
`rc2`; `rc3` to `rc5` repaired `release-linux` and the Windows upload).
`rc6` is entirely green: Linux and Windows checks, `release-linux`,
`dist-windows` (archive and installer in the GitLab package registry) and
`publier`. A `vX.Y.Z-rcN` tag does all of this, but `release-notes.sh` checks
only the crate versions there, and `publier` creates the GitLab release and then
deletes it at once (the tag stays, no release stays). The only thing still
unproven is the final content of the notes (CHANGELOG section), read at the one real tag;
`release-televerser.sh` and `release-publier.sh` can also be tried empty with
`STOKESFORGE_RELEASE_DRYRUN=1`.

## Branches and versions

Principle: **a single line as long as nothing diverges**, one more branch the
day a big work package would prevent shipping a fix. A branch
is never opened in advance: `vN` is created from `main` on the day of need,
`release/(N-1).x` from the last tag of the old major version.

```
 Phase 1 : une seule ligne            Phase 2 : un chantier qui casse
 main ──●──●──●──●──●──►              main ──●──●──●──►   V1 continue
       ▼1.0.0 ▼1.0.1 ▼1.1.0                  ↓ fusion  ↓ fusion
                                       v2 ───●──●──●──●──►  chantier cassant
```

- **`main`**: the shippable line. Fixes, small risk-free additions
  and their tags (`v1.0.1`, `v1.1.0`…) are put there directly. It is
  protected (no force-push, see `docs/tests.md` § CI (GitHub Actions for Linux and Windows)).
- **Numbers**: `Z` (`1.0.1`) = fix, or additive field of the `.stf` format
  (see `persistance.md`: risk and announcement in the CHANGELOG); `Y` (`1.1.0`) =
  feature added without breaking anything; `X` (`2.0.0`) = compatibility
  break, including any breaking change of the format (`FORMAT_VERSION`
  goes up).
- **Feature branches**: `feature/<n>-<sujet>`, one per ticket, short
  (a few days). Before merging, `main` is merged into it. It goes
  into `main` by a `--no-ff` merge, commit "Fusion #N : … (Closes #N)".
  **Never** squash or rebase a shared branch.
- **Open `vN`** (e.g. `v2`) when a work package would leave the code unusable
  for weeks (change of interface library, rework of the
  model). Criterion: *can a fix still be shipped from `main` during the
  work?* If not, branch. Adding a feature does not call for one.
  **Only one `vN` branch at a time.** While it is open, `main` receives
  only fixes and small additions; features go into
  `vN`. In `vN`, commits say "Refs #N": "Closes" closes the
  issue only at the merge into `main`.
- **The bug is fixed where it was born**: a bug present on `main` is fixed
  on `main`, in an isolated commit, and reaches `vN` by **merging** `main`
  (not by hand copy); a bug of `release/1.x` absent from `main` is
  fixed directly on `release/1.x`, without cherry-pick. Merge `main`
  into `vN` often, about every week: a long branch without merges
  gives huge conflicts. `main` → `vN` is always done by merge.
- **Every fix carries its regression test.** On a conflicting `main` → `vN`
  merge, keep the test and rewrite the fix in the code of `vN`.
- **`.stf` format frozen**: as soon as format N+1 exists on a branch, format
  N is frozen everywhere else (nothing changes in it any more), otherwise a re-saved
  file would lose data without a message.
- **End of the work package**: `vN` is merged into `main`, tag `vN.0.0`. If the
  previous version must stay maintained, then create `release/(N-1).x` from
  its last tag; the fixes born on `main` arrive there by cherry-pick
  (with the CHANGELOG line).
- **`-rcN` tags**: after V1, only before an `X` version or when CI
  or the distribution scripts have changed; always on the explicit order of the
  user. A tag is put only on `main` or on `release/N.x`: CI
  refuses it otherwise.
- **Compatibility**: internal code has no promise; the two public surfaces
  follow the numbers above: `.stf` files (invariant 12) and the
  `pyst` API, which change in a breaking way only at an `X` version.
- **Who decides `Z`, `Y` or `X`**: the user, when asking for the tag;
  the dedicated version commit carries it.
- From V1 on, **the rule "no backward compatibility of the format during
  development" no longer applies**: any change to what is written
  in a `.stf` follows the version rule of `persistance.md`.

## Out of V1

Compression and checksum of the `.stf`, `Unknown` fallback on the other
enums of the format, Windows code signing, macOS distribution: they have
no criterion here. See #323 for what can wait for V2.
