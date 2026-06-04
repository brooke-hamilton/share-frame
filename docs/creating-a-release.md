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
