"""Writes marketplace/plugins/<id>.toml for every official plugin in plugins/.

Run after changing an official plugin and bumping its version:

    cargo build -p loom-cli && python scripts/sync-marketplace.py

The content hash comes from `loom plugin check`, so the entry pins exactly the
files in the repository. Tag the release as `plugins/<id>/v<version>`.
"""
import datetime
import json
import pathlib
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOOM = ROOT / "target" / "debug" / ("loom.exe" if sys.platform == "win32" else "loom")
OUT = ROOT / "marketplace" / "plugins"


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    today = datetime.date.today().isoformat()
    for plugin in sorted(p for p in (ROOT / "plugins").iterdir() if (p / "loom-plugin.toml").is_file()):
        check = json.loads(subprocess.run([str(LOOM), "--json", "plugin", "check", str(plugin)], capture_output=True, text=True, check=True).stdout)
        manifest = tomllib.loads((plugin / "loom-plugin.toml").read_text(encoding="utf-8"))
        game = manifest.get("game", {})
        entry_path = OUT / f"{plugin.name}.toml"
        old = tomllib.loads(entry_path.read_text(encoding="utf-8")) if entry_path.exists() else {}
        versions = [v for v in old.get("versions", []) if v["version"] != check["version"]]
        versions.append({"version": check["version"], "ref": f"plugins/{plugin.name}/v{check['version']}", "sum": check["sum"], "reviewed": today})
        tags = ["official"] + (["load-order"] if game.get("load_order") else []) + (["tools"] if manifest.get("tools") else [])
        lines = [
            f'id = "{check["id"]}"',
            f'name = {json.dumps(game.get("name", manifest["plugin"]["name"]))}',
            f'kind = "{manifest["plugin"]["kind"]}"',
            f'games = {json.dumps([game["id"]] if game else [])}',
            f'description = {json.dumps(manifest["plugin"].get("description", ""))}',
            f"tags = {json.dumps(tags)}",
        ]
        for v in versions:
            lines += ["", "[[versions]]"] + [f"{k} = {json.dumps(v[k])}" for k in ("version", "ref", "sum", "reviewed")]
        entry_path.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
        print(f"{plugin.name}: {check['version']} {check['sum']}")


if __name__ == "__main__":
    main()
