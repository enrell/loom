"""Screenshots of every desktop screen, fully headless.

Serves the mock build (`npm run build:mock` in apps/desktop) on localhost and
renders each page with headless Microsoft Edge — the same engine as WebView2 —
in a throwaway profile. No window is ever shown.

    python scripts/ui-shots.py [out-dir]       # default: docs/screenshots
"""
import functools
import http.server
import pathlib
import subprocess
import sys
import tempfile
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
DIST = ROOT / "apps" / "desktop" / "dist-mock"
PAGES = ["mods", "order", "problems", "tools", "plugins", "downloads", "settings", "add-game"]
EDGE = [
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]


def main():
    out = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "docs" / "screenshots"
    out.mkdir(parents=True, exist_ok=True)
    if not (DIST / "index.html").exists():
        sys.exit("build the mock UI first: cd apps/desktop && npm run build:mock")
    edge = next((e for e in EDGE if pathlib.Path(e).exists()), None)
    if not edge:
        sys.exit("Microsoft Edge not found")
    class Quiet(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *args):
            pass

    handler = functools.partial(Quiet, directory=str(DIST))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    port = server.server_address[1]
    # The msedge.exe launcher returns at once; the browser it starts writes the
    # screenshot a moment later, so wait for each file to appear.
    with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as profile:
        for page in PAGES:
            target = out / f"{page}.png"
            target.unlink(missing_ok=True)
            subprocess.run([
                edge, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-first-run",
                f"--user-data-dir={profile}", "--window-size=1280,800", "--virtual-time-budget=4000",
                f"--screenshot={target}", f"http://127.0.0.1:{port}/?page={page}",
            ], check=True, capture_output=True, timeout=60)
            deadline = time.time() + 30
            while not (target.exists() and target.stat().st_size > 0) and time.time() < deadline:
                time.sleep(0.2)
            time.sleep(0.3)
            print(f"{page}: {target if target.exists() else 'FAILED'}")
    server.shutdown()


if __name__ == "__main__":
    main()
