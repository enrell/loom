"""Records the desktop UI's mock fixtures from a real Loom library through
the CLI, so headless UI work shows real data.

    LOOM_HOME=<library> python scripts/ui-fixtures.py <game-id>

Writes apps/desktop/mock/fixtures.json. User-specific paths are anonymized.
"""
import json
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOOM = ROOT / "target" / "debug" / ("loom.exe" if sys.platform == "win32" else "loom")
OUT = ROOT / "apps" / "desktop" / "mock" / "fixtures.json"
SAFE_HOME = r"C:\Users\you\AppData\Local\Loom"
SAFE_PROFILE = r"C:\Users\you"


def loom(*args):
    r = subprocess.run([str(LOOM), "--json", *args], capture_output=True, text=True)
    return json.loads(r.stdout)


def anonymize(value, home):
    """Replaces the library folder and the user's profile folder at the start
    of every string (paths only; import paths like github.com/<owner> stay)."""
    prefixes = [(os.path.normpath(home), SAFE_HOME), (os.path.normpath(os.path.expanduser("~")), SAFE_PROFILE)]

    def fix(s):
        tag = "path:" if s.startswith("path:") else ""
        body = s[len(tag):].replace("/", "\\")
        for prefix, repl in prefixes:
            if prefix and body.lower().startswith(prefix.lower()):
                return tag + repl + body[len(prefix):]
        return s

    if isinstance(value, dict):
        return {k: anonymize(v, home) for k, v in value.items()}
    if isinstance(value, list):
        return [anonymize(v, home) for v in value]
    return fix(value) if isinstance(value, str) else value


def main():
    game = sys.argv[1]
    home = os.environ.get("LOOM_HOME", "")
    info = loom("game", "info", game)
    profiles = loom("profile", "list", game)
    plugins = loom("plugin", "list")
    market = loom("market", "search")
    available = loom("game", "list")
    names = {e["id"]: e for e in market}
    fixtures = {
        "library": {"home": home, "games": [{"id": game, "name": info["name"], "ok": True, "profile": info["profile"]}], "plugins": len(plugins)},
        "available_games": available,
        "game_overview": {
            "id": info["id"], "name": info["name"], "path": info["path"], "profile": info["profile"], "profiles": profiles["profiles"],
            "executables": info["executables"], "nexus_domain": info["nexus_domain"], "has_load_order": info["load_order"],
            "launch_mode": info["launch_mode"], "plugin": next(p for p in plugins if p["id"] == info["plugin"]),
        },
        "mods": loom("mod", "list", game),
        "conflicts": loom("mod", "conflicts", game),
        "load_order": loom("order", "list", game),
        "doctor": loom("doctor", game),
        "tools": loom("tool", "list", game),
        "plugins": [
            {"lock": p, "name": names.get(p["id"], {}).get("name", p["id"].rsplit("/", 1)[-1]),
             "game": (names.get(p["id"], {}).get("games") or [None])[0], "tools": 0}
            for p in plugins
        ],
        "market_search": market,
        "settings": {"nexus_api_key_set": False, "launch_mode": None, "marketplace": "github.com/enrell/loom/marketplace", "mount_available": True, "home": home},
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(anonymize(fixtures, home), indent=1), encoding="utf-8")
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
