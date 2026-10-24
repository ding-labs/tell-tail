# Distribution and publishing

Tell-Tail's first public release is pending. Passing package checks does not
publish a GitHub release, update the Homebrew formula, or upload a crate.

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

## Build without publishing

Source, dependency, and packaging changes run the release workflow automatically.
To build a branch:

```sh
gh workflow run release.yml --repo zuchka/tell-tail --ref main -f publish=false
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

## Publish GitHub Releases and Homebrew

1. Merge the intended product and distribution changes, then freeze a candidate
   with matching package and lockfile versions.
2. Complete the required checks for that exact candidate and record the version,
   full SHA, results, and evidence in `packaging/validation.json`. Earlier
   candidates' evidence does not approve changed sources. See the
   [validation record](VALIDATION-0.2.md) for outstanding long-duration work.
3. Commit the evidence on `main`, then dispatch with the candidate's full SHA:

   ```sh
   gh workflow run release.yml --repo zuchka/tell-tail --ref main \
     -f publish=true -f source_commit=FULL_CANDIDATE_SHA
   ```

4. Approve the protected `release` environment. The workflow rechecks identity
   and checksums, creates a complete draft, then publishes it. Existing tags are
   never replaced. Inspect an interrupted draft before retrying.
5. After the release is public, import it into the shared tap:

   ```sh
   gh workflow run tell-tail.yml --repo ding-labs/homebrew-tap -f tag=v0.2.0
   ```

   The tap checks the published tag and artifact identity, uses that release's
   own packaging tools, and installs/tests the formula on macOS and both Linux
   architectures before updating `tell-tail.rb`. Merge the tap workflow update
   before importing the expanded Linux matrix.

Users can then run `brew install ding-labs/tap/tell-tail` and upgrade with
`brew upgrade ding-labs/tap/tell-tail`. Direct downloads live on GitHub Releases.
Present the minimum macOS version and architecture beside download links.

## Prepare the first crates.io publication

The package is named `tell-tail` and installs the `tell-tail` command. Publishing
is limited to `crates-io`. Metadata includes the license, repository, homepage,
keywords, categories, and supported documentation-build targets. Source packages
include the lockfile and embedded web assets and exclude experiments, browser
tooling, logs, and build output.

Before uploading:

1. Check availability and ownership of `tell-tail` on crates.io. No search match
   is not a reservation; hyphen/underscore variants can also conflict. Do not
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
5. After the index updates, test `cargo install tell-tail --version 0.2.0 --locked`
   in a clean directory and mark the README command as available.

Sources: [Cargo publishing](https://doc.rust-lang.org/cargo/reference/publishing.html),
[Homebrew taps](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap),
[GitHub Releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases).
