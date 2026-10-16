"""nxm:// tests.

Default: start a shell, launch a SECOND instance with an nxm link (what
Windows does when you click "Mod Manager Download" in a normal browser),
check the second one exits and the first shows the link.

--embedded: load a page in the embedded browser that clicks an nxm link
(what happens when you click "Mod Manager Download" inside Loom) and check
the shell catches it.

    python bench/nxm_test.py <shell> <out.png> [--embedded]
"""
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import psutil

from shells import ROOT, SHELLS, run_env
from snap import kill_tree, window_pid

URL = "nxm://skyrimspecialedition/mods/266/files/123456?key=abc&expires=1893456000&user_id=1"

shell, out = sys.argv[1], sys.argv[2]
cfg = SHELLS[shell]
ready = Path(tempfile.gettempdir()) / f"loom_nxm_{shell}.txt"
ready.unlink(missing_ok=True)
embedded = "--embedded" in sys.argv
page = 'data:text/html,<a id="a" href="' + URL.replace("&", "%26") + '">download</a><script>setTimeout(()=>a.click(),500)</script>'
first = psutil.Popen(cfg["run"](), cwd=cfg["cwd"], env=run_env(ready, f"url:{page}" if embedded else "page:mods"))
try:
    while not ready.exists():
        time.sleep(0.05)
    time.sleep(1)
    if embedded:
        time.sleep(3)
    else:
        t0 = time.perf_counter()
        second = subprocess.run(cfg["run"]() + [URL], cwd=cfg["cwd"], env=run_env(), timeout=30)
        print(f"{shell}: second instance exited with {second.returncode} after {time.perf_counter() - t0:.2f}s")
        time.sleep(1.5)
    subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(ROOT / "bench/shot.ps1"),
                    "-ProcessId", str(window_pid(first)), "-Out", out], check=True, capture_output=True)
finally:
    kill_tree(first)
