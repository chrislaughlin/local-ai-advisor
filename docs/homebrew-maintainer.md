# Homebrew release maintenance

The Homebrew formula lives in
[`chrislaughlin/homebrew-tap`](https://github.com/chrislaughlin/homebrew-tap).
It builds `local-ai-advisor` from a tagged source archive using the locked Cargo
dependency graph. No prebuilt binary is required.

## Publish a release

Set the version once and verify that `Cargo.toml` already contains the same
version:

```bash
VERSION=0.1.0
./scripts/verify-release.sh "$VERSION"
git status --short
git tag -a "v$VERSION" -m "local-ai-advisor v$VERSION"
git push origin main
git push origin "v$VERSION"
```

Pushing the tag runs `.github/workflows/release.yml`. It verifies the version,
builds and tests the CLI, and publishes a GitHub release with generated notes.
Wait for that workflow to succeed before updating the formula:

```bash
gh run watch --repo chrislaughlin/local-ai-advisor
gh release view "v$VERSION" --repo chrislaughlin/local-ai-advisor
```

## Update the formula

Download the exact tagged archive and calculate its SHA-256:

```bash
curl -fL \
  -o "/tmp/local-ai-advisor-$VERSION.tar.gz" \
  "https://github.com/chrislaughlin/local-ai-advisor/archive/refs/tags/v$VERSION.tar.gz"
shasum -a 256 "/tmp/local-ai-advisor-$VERSION.tar.gz"
```

In a checkout of `chrislaughlin/homebrew-tap`, update
`Formula/local-ai-advisor.rb` so `url` ends with
`refs/tags/v$VERSION.tar.gz`, replace `sha256` with the value printed above,
and remove the bootstrap `version` line. A tagged URL allows Homebrew to infer
the version.

Homebrew 6 requires formulae to be installed and audited through a tap; it
rejects path-based commands such as
`brew install ./Formula/local-ai-advisor.rb`. Commit the formula locally, then
register that checkout as the tap and validate it before pushing:

```bash
git add Formula/local-ai-advisor.rb
git commit -m "local-ai-advisor $VERSION"

brew uninstall local-ai-advisor 2>/dev/null || true
brew untap chrislaughlin/tap 2>/dev/null || true
brew tap --custom-remote chrislaughlin/tap "file://$PWD"
brew trust --formula chrislaughlin/tap/local-ai-advisor
brew install --build-from-source chrislaughlin/tap/local-ai-advisor
local-ai-advisor --version
local-ai-advisor explain
brew test chrislaughlin/tap/local-ai-advisor
brew audit --strict chrislaughlin/tap/local-ai-advisor
```

Push the formula only after those checks pass:

```bash
git push origin main
```

Finally, test both public installation forms:

```bash
brew uninstall local-ai-advisor
brew untap chrislaughlin/tap 2>/dev/null || true
brew tap chrislaughlin/tap
brew trust --formula chrislaughlin/tap/local-ai-advisor
brew install local-ai-advisor

brew uninstall local-ai-advisor
brew trust --formula chrislaughlin/tap/local-ai-advisor
brew install chrislaughlin/tap/local-ai-advisor
```
