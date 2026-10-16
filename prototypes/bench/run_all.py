"""One command for the whole comparison. Opens windows for several minutes —
run it when nobody is using the machine (and with nothing else heavy running).

    python bench/run_all.py
"""
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
SHOTS = ROOT / "docs/shots"


def py(*args):
    subprocess.run([sys.executable, *map(str, args)], cwd=ROOT, check=True)


def main():
    # 1. builds (also produce the artifacts the runtime phase launches)
    py(HERE / "bench.py", "build")
    # 2. runtime metrics
    py(HERE / "bench.py", "runtime")
    # 3. screenshots, same pages in every shell
    SHOTS.mkdir(parents=True, exist_ok=True)
    for shell in ("electron", "tauri", "egui"):
        for page, wait in (("mods", 2), ("downloads", 2), ("nexus", 9), ("media", 2), ("perf", 8)):
            py(HERE / "snap.py", shell, page, SHOTS / f"{shell}-{page}.png", "--wait", wait)
    for shell in ("electron", "tauri"):
        py(HERE / "snap.py", shell, "url:https://users.nexusmods.com/auth/sign_in", SHOTS / f"{shell}-login.png", "--wait", 9)
    # 4. nxm hand-off: from Windows (all three) and from inside the embedded browser
    for shell in ("electron", "tauri", "egui"):
        py(HERE / "nxm_test.py", shell, SHOTS / f"{shell}-nxm.png")
    for shell in ("electron", "tauri"):
        py(HERE / "nxm_test.py", shell, SHOTS / f"{shell}-nxm-embedded.png", "--embedded")
    # 5. report
    py(HERE / "report.py")


if __name__ == "__main__":
    main()
