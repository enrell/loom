"""Launch a prototype on a page, wait for first paint, screenshot it, close it.

    python bench/snap.py <shell> <page> <out.png> [--wait SECONDS]
"""
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import psutil

from shells import ROOT, SHELLS, run_env


def kill_tree(proc):
    for p in [*proc.children(recursive=True), proc]:
        try:
            p.kill()
        except psutil.NoSuchProcess:
            pass


def window_pid(root):
    """The pid in root's tree that owns the main window (PowerShell decides)."""
    pids = [root.pid] + [c.pid for c in root.children(recursive=True)]
    ps = (
        "Get-Process -Id " + ",".join(map(str, pids)) +
        " -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1 -ExpandProperty Id"
    )
    out = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True, text=True).stdout.strip()
    return int(out) if out else None


def main():
    shell, page, out = sys.argv[1], sys.argv[2], sys.argv[3]
    wait = float(sys.argv[sys.argv.index("--wait") + 1]) if "--wait" in sys.argv else 2.0
    ready = Path(tempfile.gettempdir()) / f"loom_ready_{shell}.txt"
    ready.unlink(missing_ok=True)
    cfg = SHELLS[shell]
    scenario = page if page.startswith("url:") else f"page:{page}"
    proc = psutil.Popen(cfg["run"](), cwd=cfg["cwd"], env=run_env(ready, scenario))
    try:
        deadline = time.time() + 30
        while not ready.exists() and time.time() < deadline:
            time.sleep(0.05)
        time.sleep(wait)
        pid = window_pid(proc)
        if pid is None:
            sys.exit("no window found")
        subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(ROOT / "bench/shot.ps1"),
                        "-ProcessId", str(pid), "-Out", out], check=True)
    finally:
        kill_tree(proc)


if __name__ == "__main__":
    main()
