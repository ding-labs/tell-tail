#!/usr/bin/env python3
"""Build the local macOS marketplace or a single-plugin directory submission ZIP."""
import argparse
import hashlib
import json
import platform
import shutil
import subprocess
import tempfile
from pathlib import Path
from package_release import licenses


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--submission", action="store_true",
                        help="write a plugin-only Apple Silicon ZIP for the public directory")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise SystemExit("This plugin's managed service currently supports macOS only")
    repo = Path(__file__).resolve().parents[1]
    subprocess.run(["cargo", "build", "--locked", "--release", "--bin", "tell-tail",
                    "--target-dir", str(repo / "target")], cwd=repo, check=True)
    binary = repo / "target/release/tell-tail"
    subprocess.run([str(binary), "tracing", "--help"], check=True, stdout=subprocess.DEVNULL)
    compiler = subprocess.check_output(["rustc", "-vV"], cwd=repo, text=True)
    target = next(line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: "))
    if args.submission and (target != "aarch64-apple-darwin" or
                            subprocess.check_output(["lipo", "-archs", binary], text=True).strip() != "arm64"):
        raise SystemExit("The directory listing requires an Apple Silicon binary")
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target], cwd=repo))
    notices, index = licenses(metadata)
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], cwd=repo, text=True).strip())
    notices["THIRD_PARTY_LICENSES/rust/COPYRIGHT-library.html"] = (
        (sysroot / "share/doc/rust/COPYRIGHT-library.html").read_bytes(), False)
    index.extend(["", "Rust standard library: see THIRD_PARTY_LICENSES/rust/."])
    dist = repo / "dist"
    dist.mkdir(exist_ok=True)
    name = "tell-tail-macos-" + platform.machine()
    destination = dist / name
    with tempfile.TemporaryDirectory(prefix="plugin-build-", dir=dist) as temporary:
        root = Path(temporary)
        plugin = root / "plugins/tell-tail"
        shutil.copytree(repo / "packaging/codex-plugin", plugin)
        manifest = json.loads((plugin / "plugin.json").read_text())
        (plugin / "bin").mkdir()
        shutil.copy2(binary, plugin / "bin/tell-tail")
        shutil.copy2(repo / "LICENSE", plugin / "LICENSE")
        for path, (data, _) in notices.items():
            file = plugin / path
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(data)
        (plugin / "THIRD_PARTY_NOTICES.md").write_text("\n".join(index) + "\n")
        build = {"platform": "macos", "architecture": platform.machine(),
                 "preview": not args.submission, "target": target,
                 "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
                 "source_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=repo)),
                 "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
        (plugin / "build.json").write_text(json.dumps(build, indent=2) + "\n")
        if args.submission:
            basename = f"tell-tail-plugin-{manifest['version']}-macos-arm64"
            archive = shutil.make_archive(str(root / basename), "zip", plugin)
            output = dist / (basename + ".zip")
            Path(archive).replace(output)
            print(json.dumps({"submission": str(output), "source_commit": build["source_commit"],
                              "archive_sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                              "binary_sha256": build["sha256"]}, indent=2))
            return
        marketplace = root / ".agents/plugins"
        marketplace.mkdir(parents=True)
        (marketplace / "marketplace.json").write_text(json.dumps({
            "name": "tell-tail-local", "plugins": [{"name": "tell-tail",
            "source": {"source": "local", "path": "./plugins/tell-tail"},
            "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
            "category": manifest["extensions"]["com.openai"]["interface"]["category"]}]}, indent=2) + "\n")
        # Only replace this generated build directory; never touch the user's plugin cache.
        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(root, destination)
    archive = shutil.make_archive(str(dist / name), "zip", destination)
    print(json.dumps({"marketplace": str(destination), "archive": archive,
                      "binary_sha256": build["sha256"]}, indent=2))


if __name__ == "__main__":
    main()
