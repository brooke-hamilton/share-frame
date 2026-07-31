---
name: create-release
description: 'Cut a new share-frame release by executing the process in docs/creating-a-release.md: work out and validate the next version, run the doc''s tagging steps, then monitor and, if needed, recover the Release workflow. USE FOR: "create a release", "cut a release", "publish a new version", "release v0.1.2", "tag a release", "the release workflow failed", "recover a failed release". DO NOT USE FOR: general git tagging unrelated to releases, editing .github/workflows/release.yml, or repositories other than share-frame.'
---

# Create a share-frame Release

Drive a release of `share-frame`. **The single source of truth for the actual commands and their
rationale is [`docs/creating-a-release.md`](../../../docs/creating-a-release.md).** This skill only
adds the agent-side orchestration around that doc — version selection, preflight checks, a
confirmation gate, and run monitoring. Do not restate the doc's commands here; open the doc and run
its steps verbatim, substituting the version you determine below.

If the doc and this skill ever disagree, the doc wins — fix the skill.

## When to use

Use when the user wants to ship a new version, or to recover a release run that failed. Do not use
for editing the workflow or for unrelated tagging.

## What this skill owns (vs. the doc)

- **This skill:** choose/validate the version, run preflight checks, confirm before the public
  push, watch the workflow, and decide which recovery branch applies.
- **The doc:** the exact `git`/`cargo` commands, why the tag must be signed, and the recovery
  command sequences. Read [`docs/creating-a-release.md`](../../../docs/creating-a-release.md) and
  execute the step it describes at each point below.

## Procedure

1. **Determine the target version.**
   - Read the current `version` from `Cargo.toml` (`[package]` → `version = "X.Y.Z"`).
   - If the user did not give a version, offer patch / minor / major bumps computed from it as
     concrete choices. Validate the result is semver `X.Y.Z` and strictly greater than current.

2. **Preflight (things the doc assumes but does not check).**
   - A signing key is configured (`git config user.signingkey`); the doc requires a signed tag, so
     stop and tell the user if none is set.
   - The version bump will land on `main`. If the current branch is a worktree/feature branch,
     confirm with the user how it reaches `main` before continuing.
   - `gh` is authenticated (needed to watch the run).

3. **Run the doc's "Steps" section** in
   [`docs/creating-a-release.md`](../../../docs/creating-a-release.md#steps): bump `Cargo.toml`,
   commit, push to `main`, then create and push the signed `vX.Y.Z` tag and verify it. Use the
   version from step 1. If `Cargo.lock` is tracked, sync it (e.g. `cargo build`) before committing
   so the workflow's `--locked` build succeeds.
   - **Before pushing the tag,** show the user the exact version and commit SHA being tagged and get
     explicit confirmation — the tag push starts a public release.

4. **Monitor the Release workflow** (the doc ends once the tag is pushed; this part is on the agent).
   Filter by the tag you just pushed so you watch the run for this release and not an unrelated one
   (for a tag push, the run's head branch is the tag name):

   ```pwsh
   gh run list --workflow release.yml --branch vX.Y.Z --limit 1
   gh run watch <run-id> --exit-status
   ```

   On success, report the release: `gh release view vX.Y.Z`.

5. **If the run fails,** follow the doc's
   [recovery section](../../../docs/creating-a-release.md#recovering-from-a-failed-release-workflow).
   First determine which case applies (`gh release view vX.Y.Z` tells you whether a Release was
   published):
   - **No Release published** → keep the version and follow the doc's retag steps, then repeat
     step 4.
   - **Release already published** → do not reuse the tag; return to step 1 with a bumped version.

## Completion checklist

- [ ] Signed tag `vX.Y.Z` matches the pushed `Cargo.toml` version and verifies (`git tag -v`).
- [ ] The `Release` workflow run completed successfully.
- [ ] The GitHub Release for `vX.Y.Z` exists with the zip + `.sha256` artifacts attached.
