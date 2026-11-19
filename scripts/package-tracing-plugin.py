#!/usr/bin/env python3
"""Build a self-contained macOS plugin and local marketplace; never install it."""
import hashlib
import json
import platform
import shutil
import subprocess
import tempfile
from pathlib import Path


def main():
    if platform.system() != "Darwin":
        raise SystemExit("This plugin's managed service currently supports macOS only")
    repo = Path(__file__).resolve().parents[1]
    subprocess.run(["cargo", "build", "--locked", "--release", "--bin", "tell-tail",
                    "--target-dir", str(repo / "target")], cwd=repo, check=True)
    binary = repo / "target/release/tell-tail"
    subprocess.run([str(binary), "tracing", "--help"], check=True, stdout=subprocess.DEVNULL)
    dist = repo / "dist"
    dist.mkdir(exist_ok=True)
    name = "tell-tail-macos-" + platform.machine()
    destination = dist / name
    with tempfile.TemporaryDirectory(prefix="plugin-build-", dir=dist) as temporary:
        root = Path(temporary)
        plugin = root / "plugins/tell-tail"
        shutil.copytree(repo / "packaging/codex-plugin", plugin)
        (plugin / "bin").mkdir()
        shutil.copy2(binary, plugin / "bin/tell-tail")
        shutil.copy2(repo / "LICENSE", plugin / "LICENSE")
        shutil.copytree(repo / "packaging/licenses", plugin / "licenses")
        build = {"platform": "macos", "architecture": platform.machine(),
                 "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
        (plugin / "build.json").write_text(json.dumps(build, indent=2) + "\n")
        marketplace = root / ".agents/plugins"
        marketplace.mkdir(parents=True)
        (marketplace / "marketplace.json").write_text(json.dumps({
            "name": "tell-tail-local", "plugins": [{"name": "tell-tail",
            "source": {"source": "local", "path": "./plugins/tell-tail"},
            "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
            "category": "Productivity"}]}, indent=2) + "\n")
        # Only replace this generated build directory; never touch the user's plugin cache.
        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(root, destination)
    archive = shutil.make_archive(str(dist / name), "zip", destination)
    print(json.dumps({"marketplace": str(destination), "archive": archive,
                      "binary_sha256": build["sha256"]}, indent=2))


if __name__ == "__main__":
    main()
