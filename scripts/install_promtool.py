#!/usr/bin/env python3
"""Install a checksum-pinned promtool for validation (no server is started)."""
import hashlib
import pathlib
import platform
import shutil
import tarfile
import tempfile
import urllib.request

VERSION = '3.3.0'
# Published https://github.com/prometheus/prometheus/releases/download/v3.3.0/sha256sums.txt
CHECKSUMS = {
    'darwin-amd64': '7b45e8ca59df6e4819d023b3e1137d89f9341c94948f861ff54c45320bbeb216',
    'darwin-arm64': 'e06d039b9f70aa8bbf0bfb0a5a93b773bd5b9b94dc8b4f2aec278a1a9b8a5bba',
    'linux-amd64': 'cba98e0e17edab9f745122d92072e7de59440da9c975c51ecf982b0b2d630787',
    'linux-arm64': 'ac84be2c78dad8c9b808a51dcded7009c7c1ef382675a777ce5fd729535178cd',
}
architecture = {'x86_64': 'amd64', 'aarch64': 'arm64', 'arm64': 'arm64'}[platform.machine()]
target = platform.system().lower() + '-' + architecture
name = f'prometheus-{VERSION}.{target}'
destination = pathlib.Path(__file__).resolve().parents[1] / 'work/tools'
destination.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(dir=destination) as temporary:
    archive = pathlib.Path(temporary) / 'release.tar.gz'
    url = f'https://github.com/prometheus/prometheus/releases/download/v{VERSION}/{name}.tar.gz'
    digest = hashlib.sha256()
    with urllib.request.urlopen(url, timeout=60) as response, archive.open('wb') as output:
        for block in iter(lambda: response.read(1024**2), b''):
            digest.update(block)
            output.write(block)
    assert digest.hexdigest() == CHECKSUMS[target], 'release checksum mismatch'
    with tarfile.open(archive) as tar:
        member = tar.getmember(name + '/promtool')
        assert member.isfile()
        with tar.extractfile(member) as source, (destination / 'promtool').open('wb') as output:
            shutil.copyfileobj(source, output)
    (destination / 'promtool').chmod(0o755)
print(destination / 'promtool')
