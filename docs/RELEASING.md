# Distribution and publishing

Tell-Tail 0.2.0 was published to crates.io from commit
`02a6476f9921ba45c59a6ce98bd68b317bac79b6` on October 28, 2026.
The first GitHub release and Homebrew publication remain pending. Passing
package checks does not publish to any channel.

## Supported release artifacts

| Archive target | Runtime | Build and test host |
| --- | --- | --- |
| `aarch64-apple-darwin` | Apple Silicon, macOS 14+ | Native macOS ARM64 |
| `x86_64-unknown-linux-musl` | 64-bit x86 Linux, static musl | Native Linux x86_64 |
| `aarch64-unknown-linux-musl` | 64-bit ARM Linux, static musl | Native Linux ARM64 |

Linux archives have no dynamic loader or shared-library dependencies. Packaging
rejects a binary for the wrong architecture or one with dynamic dependencies.
The workflow tests each archive in Debian 11 and Alpine 3.22 userspaces as well
as on its Ubuntu build host. Containers share the host kernel; these checks do
not establish a minimum supported kernel version.

Every archive includes the executable, license, third-party notices and license
texts, README, changelog, and `BUILD.json` with its source commit, target, compiler,
and linkage. The dashboard is embedded; users do not need Rust, Node, or Python.
`release.json` describes all three archives and `SHA256SUMS` covers those archives
and the manifest. The Homebrew formula installs the same tested binaries.

macOS binaries currently have the linker's ad-hoc signature. Developer ID
signing and notarization are not configured. Native and Homebrew smoke checks
do not test a browser download carrying a quarantine attribute. Plan and test
that installation path before advertising direct macOS downloads; see
[Apple's distribution guidance](https://developer.apple.com/developer-id/).

## Build without publishing

Source, dependency, and packaging changes run the release workflow automatically
on pull requests and pushes to main. Superseded build-only runs are cancelled;
publication runs remain serialized and are never cancelled by a new build.
To build a branch:

```sh
gh workflow run release.yml --repo ding-labs/tell-tail --ref main -f publish=false
```

Replace `main` with the branch to check. Omit `source_commit` to use that
workflow's commit, or provide a full SHA. Build-only runs work off main;
publication requires a commit reachable from `origin/main` and a run on `main`.
Download the `release-candidate` Actions artifact to inspect the archives,
manifest, checksums, and generated `tell-tail.rb`. Artifacts last 14 days.

Local packaging checks require Python 3.11+ and the pinned Rust 1.88.0 toolchain:

```sh
python3.11 -m unittest discover -s scripts -p 'test_release*.py'
python3.11 scripts/test_package_release.py
python3.11 scripts/test_render_formula.py
python3.11 scripts/check_package.py
```

The last command performs `cargo publish --dry-run --locked`, inspects the
source package, installs it offline into a fresh directory, and exercises JSON
output and the embedded dashboard. Use `--allow-dirty` only for a local preview;
the report records it. Native release packaging requires a clean checkout.

## One-time automation setup

The `Release` workflow in `.github/workflows/release.yml` publishes all three
channels from one approved candidate. Configure these integrations before the
first automated publication:

1. In the [tell-tail crate settings](https://crates.io/crates/tell-tail/settings),
   add a GitHub trusted publisher with owner `ding-labs`, repository `tell-tail`,
   workflow filename `release.yml`, and environment `release`. The workflow uses
   GitHub OIDC and the pinned official `rust-lang/crates-io-auth-action` to obtain
   a temporary publishing token. Do not add a permanent crates.io token to CI.
   The crate already exists, so manual first-publication bootstrap is complete.
2. Create a fine-grained GitHub token with resource owner `ding-labs`, access to
   **only** `homebrew-tap`, and **Actions: read and write**. Save it as
   `HOMEBREW_TAP_TOKEN` in the **tell-tail repository's `release` environment**.
   The token dispatches and observes the tap's existing import workflow; that
   workflow uses its own `GITHUB_TOKEN` to commit the tested formula. The dispatch
   token needs no Contents write permission. Grant organization approval if
   required, and renew the token before its chosen expiration.
3. Keep the `release` environment restricted to `main` with the required
   maintainer review. The workflow has `contents: write` for GitHub Releases and
   `id-token: write` for crates.io only in the protected publication job.

No CI publication credentials are needed for build-only runs. A missing tap
secret, an inaccessible import workflow, or a failed crates.io authentication
stops the publication job before it creates a GitHub release.

## Publish all three channels

1. Merge the intended product and distribution changes, then freeze a candidate
   with matching package and lockfile versions. It must be reachable from `main`.
2. Complete the required checks for that exact candidate and record the version,
   full SHA, results, and evidence in `packaging/validation.json`. Earlier
   candidates' evidence does not approve changed sources. See the
   [validation record](VALIDATION-0.2.md) for the historical campaign; newer
   candidate evidence must identify its own source. Automation does not waive
   the existing validation requirements.
3. Commit the evidence on `main`, then dispatch:

   ```sh
   gh workflow run release.yml --repo ding-labs/tell-tail --ref main \
     -f publish=true -f source_commit=FULL_CANDIDATE_SHA
   ```

4. After all three native build/archive/Homebrew smoke jobs and bundle checks
   pass, approve the protected `release` environment. One protected job then:

   - Rechecks validation, candidate identity and archive checksums.
   - Checks Homebrew credentials and any existing crates.io version, verifies a
     clean source-package install including dashboard assets, and authenticates
     to crates.io if a new upload is needed.
   - Creates a complete GitHub draft, uploads the archives, manifest and
     checksums, publishes it, and verifies the published assets.
   - Publishes the same source/version to crates.io, verifies its registry
     checksum and embedded Git commit, then installs and smoke-tests it.
   - Dispatches `tell-tail.yml` on `ding-labs/homebrew-tap` for the stable release
     tag and waits for that exact run to finish. The tap validates the release
     and installs/tests its formula on macOS ARM64 and both Linux architectures
     before committing `tell-tail.rb` to the tap's `main`.

5. Read the publication job's channel-results summary and the linked Homebrew
   run. A successful release run means all three channels completed, including
   the tap update. A dispatched tap job alone is not reported as success.

Users can then install using `cargo install tell-tail --locked` or
`brew install ding-labs/tap/tell-tail`, upgrade with
`brew upgrade ding-labs/tap/tell-tail`, or download archives from GitHub Releases.
Present the minimum macOS version and architecture beside download links.
Developer ID signing and browser quarantine validation remain separate from this
workflow; the unsigned direct Mac download path is not certified by these checks.

### Recover a partial publication

Publication across services is sequential, not atomic. A later channel can fail
after an earlier one is public. Fix the integration or service failure, then use
GitHub Actions **Re-run failed jobs** on the same run to preserve its candidate
and evidence. Do not bump the version just to retry Homebrew.

- A published GitHub release is reused only if its commit tag and checksummed
  artifacts identify the requested source/version. Existing tags and public
  assets are never overwritten. A partial draft uploads only missing files;
  different existing draft files or another target require manual inspection.
- An existing crates.io version is skipped only after checking that it is not
  yanked and that the downloaded crate's checksum, manifest and Git source match
  the candidate. A conflicting version stops before GitHub publication. In
  particular, the already published 0.2.0 is reusable only for its original
  `02a6476f9921ba45c59a6ce98bd68b317bac79b6` source.
- Homebrew imports may be dispatched again. The tap does not create another
  commit when the tested formula is unchanged. If a previous import times out,
  inspect its printed run URL before retrying; the original import may still
  finish. Failures, cancellation and timeout keep the release job unsuccessful.

The orchestration helpers are tested offline with simulated service responses;
CI also runs those tests. Live publication still requires the settings above,
passing exact-candidate validation and environment approval.

## Manual crates.io publication

Use the unified workflow above for normal releases. These commands remain useful
for bootstrap or recovery when manual publication is explicitly authorized.

The package is named `tell-tail` and installs the `tell-tail` command. Publishing
is limited to `crates-io`. Metadata includes the license, repository, homepage,
keywords, categories, and supported documentation-build targets. Source packages
include the lockfile and embedded web assets and exclude experiments, browser
tooling, logs, and build output.

Before uploading:

1. Check availability and ownership of `tell-tail` on crates.io. A search without
   matches does not reserve a name; hyphen/underscore variants can also conflict. Do not
   rename the package without updating the binary/install docs.
2. Set up a crates.io account with a verified email and publishing credentials.
   Use `cargo login` interactively; keep credentials out of repository files and
   command history. Initial manual publication needs no CI token.
3. Use the same validated candidate as the GitHub release in a clean checkout.
   Review the README, changelog, version, and package contents:

   ```sh
   cargo package --list --locked
   python3.11 scripts/check_package.py
   ```

4. When publication is authorized, run `cargo publish --locked --registry crates-io`
   from that candidate. Preparation never invokes this command. Registry
   versions cannot be overwritten, so review before the first upload.
5. After the index updates, test `cargo install tell-tail --version VERSION --locked`
   in a clean directory. Keep the installation documentation current.

Sources: [crates.io trusted publishing](https://crates.io/docs/trusted-publishing),
[GitHub workflow dispatch API](https://docs.github.com/en/rest/actions/workflows#create-a-workflow-dispatch-event),
[Cargo publishing](https://doc.rust-lang.org/cargo/reference/publishing.html),
[Homebrew taps](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap),
[GitHub Releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases).
