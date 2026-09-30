"""Validate the archive submitted to GNOME Extensions."""

import json
import sys
import xml.etree.ElementTree as ET
import zipfile


with zipfile.ZipFile(sys.argv[1]) as archive:
    expected = {
        "extension.js", "prefs.js", "metadata.json", "stylesheet.css", "LICENSE",
        "schemas/org.gnome.shell.extensions.agent-notifier.gschema.xml",
    }
    files = {name for name in archive.namelist() if not name.endswith("/")}
    assert files == expected, f"Unexpected extension archive files: {files ^ expected}"
    metadata = json.loads(archive.read("metadata.json"))
    assert metadata["uuid"] == "agent-notifier@mmmohebi.github.io"
    assert metadata["settings-schema"] == "org.gnome.shell.extensions.agent-notifier"
    assert "clipboard" in metadata["description"].lower()
    assert "companion" in metadata["description"].lower()
    schema = ET.fromstring(archive.read("schemas/org.gnome.shell.extensions.agent-notifier.gschema.xml")).find("schema")
    assert schema.attrib["id"] == metadata["settings-schema"]
    assert schema.attrib["path"] == "/org/gnome/shell/extensions/agent-notifier/"
    for name in files:
        assert not archive.read(name).startswith(b"\x7fELF"), f"Bundled executable: {name}"
    assert archive.testzip() is None, "Corrupt extension archive"

print("GNOME extension archive metadata, schema, licensing file, and contents are valid")
