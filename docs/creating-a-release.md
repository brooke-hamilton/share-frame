# Creating a Release

Releases are built and published automatically by the [`Release` workflow](../.github/workflows/release.yml) when you push a version tag.

## Steps

1. **Bump the version** in [`Cargo.toml`](../Cargo.toml) (the `version` field) and commit it to `main`.

   ```pwsh
   git commit -am "Release v0.1.0"
   git push origin main
   ```

2. **Create and push a matching tag** (the tag must be `v` + the `Cargo.toml` version, or the build fails).

   Use a signed tag (`-s`) so the tag's provenance can be cryptographically verified. This requires a configured signing key (`git config user.signingkey ...`); GitHub displays a **Verified** badge for tags signed with a key you've added to your account.

   ```pwsh
   git tag -s v0.1.0 -m "Release v0.1.0"
   git push origin v0.1.0
   ```

   Verify the signature locally with:

   ```pwsh
   git tag -v v0.1.0
   ```

That's it. The workflow builds x64 and arm64 binaries, packages each as a zip with a SHA-256 checksum, and publishes a GitHub Release with auto-generated notes.

## Recovering from a failed release workflow

If the `Release` workflow fails (for example, a build error) **before a GitHub Release was published**, you can fix the problem and reuse the same version number — there's no need to bump it, because nothing was released to the public yet. (Only bump the version if a release was already published for it; published tags should stay immutable.)

The workflow builds whatever commit the tag points to, so after committing your fix you must move the tag to the new commit. Because the tag already exists locally and on the remote, you have to delete and recreate it.

1. **Commit and push the fix** to `main`, then make sure your local `main` is up to date.

   ```pwsh
   git switch main
   git pull
   ```

2. **Delete the old tag** locally and on the remote.

   ```pwsh
   git tag -d v0.1.0
   git push origin :refs/tags/v0.1.0
   ```

3. **Recreate the signed tag** on the fixed commit and push it. Pushing the tag re-triggers the `Release` workflow.

   ```pwsh
   git tag -s v0.1.0 -m "Release v0.1.0"
   git push origin v0.1.0
   ```

> If a GitHub Release *was* already published for the version, do **not** reuse the tag. Bump the version in [`Cargo.toml`](../Cargo.toml) instead and follow the normal release steps above with the new version number.

