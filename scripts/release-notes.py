"""Print required changelog notes for the current package or a requested release."""

from pathlib import Path
import re
import sys


def release_notes(changelog, version):
    heading = re.compile(r"^## \[([^\]]+)\] - \d{4}-\d{2}-\d{2}\s*$", re.MULTILINE)
    matches = list(heading.finditer(changelog))
    entries = [index for index, match in enumerate(matches) if match.group(1) == version]
    if len(entries) != 1:
        raise ValueError(f"CHANGELOG.md must contain exactly one dated entry for {version}")
    index = entries[0]
    end = matches[index + 1].start() if index + 1 < len(matches) else len(changelog)
    body = changelog[matches[index].end():end].strip()
    if not re.search(r"^[-*] \S", body, re.MULTILINE):
        raise ValueError(f"CHANGELOG.md entry for {version} must describe at least one change")
    return body


def main():
    project = Path(__file__).resolve().parent.parent
    manifest = (project / "Cargo.toml").read_text()
    package = re.search(r"(?ms)^\[package\]\s*\n(.*?)(?=^\[|\Z)", manifest)
    package_version = re.search(r'^version\s*=\s*"([^"]+)"', package.group(1), re.MULTILINE).group(1)
    version = sys.argv[1] if len(sys.argv) > 1 else package_version
    if version != package_version:
        raise ValueError(f"Release {version} does not match Cargo.toml version {package_version}")
    print(release_notes((project / "CHANGELOG.md").read_text(), version))


if __name__ == "__main__":
    try:
        main()
    except ValueError as error:
        sys.exit(str(error))
