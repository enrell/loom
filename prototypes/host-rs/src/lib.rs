//! Host-side helpers shared by the Tauri and egui prototypes, so both
//! report metrics exactly the same way.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Milliseconds since the OS created this process (not since `main`), so
/// loader and runtime start-up are included — the same clock Electron's
/// `process.getCreationTime()` uses.
pub fn ms_since_process_creation() -> f64 {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let ft = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
    let (mut created, mut exited, mut kernel, mut user, mut now) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: all out-pointers are valid FILETIMEs; the pseudo handle is always valid.
    unsafe {
        GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user);
        GetSystemTimePreciseAsFileTime(&mut now);
    }
    (ft(now).saturating_sub(ft(created))) as f64 / 10_000.0
}

static READY: OnceLock<f64> = OnceLock::new();

/// Records first paint once; writes `LOOM_BENCH_READY_FILE` for the harness.
pub fn mark_ready() -> f64 {
    *READY.get_or_init(|| {
        let ms = ms_since_process_creation();
        if let Ok(path) = std::env::var("LOOM_BENCH_READY_FILE") {
            let _ = std::fs::write(path, format!("{ms}"));
        }
        ms
    })
}

/// Startup time if ready was marked, else time so far.
pub fn startup_ms() -> f64 {
    READY.get().copied().unwrap_or_else(ms_since_process_creation)
}

/// `LOOM_BENCH_WINDOW=x,y`: test runs open the window there, without
/// activating it, so they never cover or take focus from the user's work.
pub fn bench_window() -> Option<(f64, f64)> {
    let v = std::env::var("LOOM_BENCH_WINDOW").ok()?;
    let (x, y) = v.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

pub fn bench_scenario() -> Option<String> {
    std::env::var("LOOM_BENCH_SCENARIO").ok().filter(|s| !s.is_empty())
}

/// `nxm://` links passed on the command line.
pub fn nxm_args(args: impl IntoIterator<Item = String>) -> Vec<String> {
    args.into_iter().filter(|a| a.to_ascii_lowercase().starts_with("nxm://")).collect()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    pub working_set_mb: f64,
    /// Sum over the process tree, percent of ONE core (like Electron's
    /// `percentCPUUsage` and psutil's `cpu_percent`).
    pub cpu_percent: f64,
    pub processes: usize,
}

/// Samples this process and all its descendants (WebView2 helpers
/// included). Re-discovers the tree every 5 s; refreshes only those pids
/// in between so the sampler itself stays cheap.
pub struct Sampler {
    sys: System,
    tree: Vec<Pid>,
    discovered: Option<Instant>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        Self { sys: System::new(), tree: Vec::new(), discovered: None }
    }

    fn discover(&mut self) {
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        let me = Pid::from_u32(std::process::id());
        let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
        for (pid, p) in self.sys.processes() {
            if let Some(parent) = p.parent() {
                children.entry(parent).or_default().push(*pid);
            }
        }
        let mut seen = HashSet::new();
        let mut stack = vec![me];
        while let Some(pid) = stack.pop() {
            if seen.insert(pid) {
                stack.extend(children.get(&pid).into_iter().flatten().copied());
            }
        }
        self.tree = seen.into_iter().collect();
        self.discovered = Some(Instant::now());
    }

    pub fn sample(&mut self) -> Metrics {
        if self.discovered.is_none_or(|t| t.elapsed() > Duration::from_secs(5)) {
            self.discover();
        }
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&self.tree),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let mut m = Metrics::default();
        for pid in &self.tree {
            if let Some(p) = self.sys.process(*pid) {
                m.working_set_mb += p.memory() as f64 / (1024.0 * 1024.0);
                m.cpu_percent += f64::from(p.cpu_usage());
                m.processes += 1;
            }
        }
        m
    }
}

/// Registers `exe` as this user's `nxm://` handler (HKCU, no admin) — the
/// same keys Electron's `setAsDefaultProtocolClient` writes.
pub fn register_nxm(exe: &Path) -> Result<String, String> {
    let exe = exe.display().to_string();
    let reg = |args: &[&str]| -> Result<(), String> {
        let out = std::process::Command::new("reg").args(args).output().map_err(|e| e.to_string())?;
        if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()) }
    };
    let base = r"HKCU\Software\Classes\nxm";
    reg(&["add", base, "/ve", "/d", "URL:nxm", "/f"])?;
    reg(&["add", base, "/v", "URL Protocol", "/d", "", "/f"])?;
    reg(&["add", &format!(r"{base}\shell\open\command"), "/ve", "/d", &format!("\"{exe}\" \"%1\""), "/f"])?;
    Ok(format!("Registered {exe} as the nxm:// handler for this user."))
}
