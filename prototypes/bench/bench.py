"""Benchmarks the three shells end to end and writes bench/results/latest.json.

    python bench/bench.py build     # compile time + build memory/CPU
    python bench/bench.py runtime   # startup, idle/scroll/video/nexus memory + CPU, size
    python bench/bench.py all

Every command runs inside a Windows Job Object (see jobs.py), so CPU time and
peak memory cover *all* descendant processes: rustc/cargo/node during builds,
renderer/GPU/WebView2 helpers at runtime.
"""
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import psutil

from jobs import Job
from shells import ROOT, SHELLS, build_env, run_env

RESULTS = ROOT / "bench/results"
MB = 1024 * 1024


def tree_memory(pids):
    """(working set, private bytes) summed over pids, in MB."""
    ws = priv = 0
    for pid in pids:
        try:
            mi = psutil.Process(pid).memory_info()
            ws += mi.rss
            priv += mi.private
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            pass
    return ws / MB, priv / MB


def du(path):
    path = Path(path)
    if path.is_file():
        return path.stat().st_size
    return sum(p.stat().st_size for p in path.rglob("*") if p.is_file())


# ---------------------------------------------------------------- builds

def timed_build(cmd, cwd, label):
    """Runs one build in a job; samples tree working set every 200 ms."""
    print(f"  {label}: {' '.join(cmd)}", flush=True)
    # CreateProcess doesn't search PATH for .cmd shims (npm.cmd): resolve here.
    cmd = [shutil.which(cmd[0]) or cmd[0], *cmd[1:]]
    t0 = time.perf_counter()
    # Output goes to a file: a pipe nobody reads until the end fills up
    # (cargo prints a line per crate) and blocks the build.
    log = tempfile.TemporaryFile()
    job = Job.spawn(cmd, cwd=cwd, env=build_env(), stdout=log, stderr=subprocess.STDOUT)
    peak_ws = 0.0
    while job.proc.poll() is None:
        ws, _ = tree_memory(job.pids())
        peak_ws = max(peak_ws, ws)
        time.sleep(0.2)
    wall = time.perf_counter() - t0
    log.seek(0)
    err = log.read().decode(errors="replace")
    log.close()
    if job.proc.returncode != 0:
        print(err[-3000:])
        raise SystemExit(f"{label} failed ({job.proc.returncode})")
    r = {
        "wall_s": round(wall, 2),
        "cpu_s": round(job.cpu_seconds(), 1),
        "peak_commit_mb": round(job.peak_commit_bytes() / MB),
        "peak_working_set_mb": round(peak_ws),
        "processes": job.total_processes(),
    }
    job.close()
    print(f"    {r}", flush=True)
    return r


def touch(path):
    path = Path(path)
    path.write_text(path.read_text(encoding="utf-8"), encoding="utf-8")
    os.utime(path, None)


def bench_build(shell):
    cfg = SHELLS[shell]
    out = {}
    for p in cfg["clean"]:
        shutil.rmtree(p, ignore_errors=True)
    out["clean_release"] = timed_build(cfg["build"], cfg["cwd"], f"{shell} clean release")
    touch(cfg["touch"])
    out["incremental_release"] = timed_build(cfg["build"], cfg["cwd"], f"{shell} incremental release")
    if "debug" in cfg:
        cargo_dir, cmd = cfg["debug"]
        out["clean_debug"] = timed_build(cmd, cargo_dir, f"{shell} clean debug")
        touch(cfg["touch"])
        out["incremental_debug"] = timed_build(cmd, cargo_dir, f"{shell} incremental debug")
    out["artifact_mb"] = round(du(cfg["artifact"]) / MB, 1)
    return out


# ---------------------------------------------------------------- runtime

def launch(shell, scenario=None):
    ready = Path(tempfile.gettempdir()) / f"loom_bench_ready_{shell}.txt"
    ready.unlink(missing_ok=True)
    cfg = SHELLS[shell]
    t0 = time.perf_counter()
    job = Job.spawn(cfg["run"](), cwd=cfg["cwd"], env=run_env(ready, scenario))
    while not ready.exists():
        if time.perf_counter() - t0 > 60:
            job.terminate()
            raise SystemExit(f"{shell} never became ready")
        time.sleep(0.002)
    external_ms = (time.perf_counter() - t0) * 1000
    time.sleep(0.05)
    internal_ms = float(ready.read_text() or "nan")
    return job, external_ms, internal_ms


def stop(job):
    job.terminate()
    job.close()
    time.sleep(1.5)  # let WebView2/Chromium release their profile locks


def bench_startup(shell, runs):
    ext, internal = [], []
    for i in range(runs + 1):
        job, e, n = launch(shell)
        stop(job)
        if i:  # first run warms the disk cache; discarded
            ext.append(e)
            internal.append(n)
    r = {
        "external_ms_median": round(statistics.median(ext)),
        "external_ms_min": round(min(ext)),
        "in_app_ms_median": round(statistics.median(internal)),
        "runs": runs,
    }
    print(f"  {shell} startup {r}", flush=True)
    return r


def bench_scenario(shell, scenario, settle, window):
    job, _, _ = launch(shell, scenario)
    time.sleep(settle)
    cpu0, t0 = job.cpu_seconds(), time.perf_counter()
    ws_samples, priv_samples, procs = [], [], 0
    while time.perf_counter() - t0 < window:
        pids = job.pids()
        ws, priv = tree_memory(pids)
        ws_samples.append(ws)
        priv_samples.append(priv)
        procs = max(procs, len(pids))
        time.sleep(0.5)
    cpu = (job.cpu_seconds() - cpu0) / (time.perf_counter() - t0) * 100
    stop(job)
    r = {
        "cpu_percent_mean": round(cpu, 2),
        "working_set_mb_mean": round(statistics.mean(ws_samples)),
        "working_set_mb_peak": round(max(ws_samples)),
        "private_mb_mean": round(statistics.mean(priv_samples)),
        "processes": procs,
    }
    print(f"  {shell} {scenario} {r}", flush=True)
    return r


SCENARIOS = [
    # (name, LOOM_BENCH_SCENARIO, settle s, window s, shells that support it)
    ("idle", "page:mods", 5, 20, ("electron", "tauri", "egui", "egui-glow")),
    ("scroll", "scroll", 3, 15, ("electron", "tauri", "egui", "egui-glow")),
    ("video", "video", 3, 15, ("electron", "tauri")),
    ("nexus", "page:nexus", 15, 15, ("electron", "tauri")),
]


def bench_runtime(shell, runs):
    out = {"startup": bench_startup(shell, runs)}
    for name, scenario, settle, window, shells in SCENARIOS:
        out[name] = bench_scenario(shell, scenario, settle, window) if shell in shells else None
    return out


# ---------------------------------------------------------------- main

def machine():
    import platform
    cpu = subprocess.run(["powershell", "-NoProfile", "-Command", "(Get-CimInstance Win32_Processor).Name"],
                         capture_output=True, text=True).stdout.strip()
    return {"os": platform.platform(), "cpu": cpu, "logical_cpus": psutil.cpu_count(),
            "ram_gb": round(psutil.virtual_memory().total / 2**30)}


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else "all"
    shells = [s for s in sys.argv[2:] if s in SHELLS] or list(SHELLS)
    runs = int(os.environ.get("LOOM_BENCH_RUNS", "5"))
    RESULTS.mkdir(parents=True, exist_ok=True)
    latest = RESULTS / "latest.json"
    data = json.loads(latest.read_text()) if latest.exists() else {}
    data["machine"] = machine()
    for shell in shells:
        entry = data.setdefault(shell, {})
        if what in ("build", "all"):
            print(f"[build] {shell}", flush=True)
            entry["build"] = bench_build(shell)
        if what in ("runtime", "all"):
            print(f"[runtime] {shell}", flush=True)
            entry["runtime"] = bench_runtime(shell, runs)
        latest.write_text(json.dumps(data, indent=2))
    data["generated"] = time.strftime("%Y-%m-%d %H:%M")
    latest.write_text(json.dumps(data, indent=2))
    print(f"wrote {latest}")


if __name__ == "__main__":
    main()
