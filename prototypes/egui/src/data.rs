//! Same deterministic generator as ui/app.js — same LCG, lists and call
//! order — so all three shells show the same 2000 mods.

const PREFIX: &[&str] = &[
    "Immersive", "Enhanced", "Unofficial", "Realistic", "Better", "Ultimate", "Simple", "Dynamic", "Legacy of",
    "Vanilla Plus", "HD", "Lore-Friendly", "Interesting", "Classic", "Static Mesh",
];
const SUBJECT: &[&str] = &[
    "Armor", "Weapons", "Lighting", "Water", "Landscapes", "Cities", "Followers", "Combat", "Dragons", "Interiors",
    "Sounds", "Textures", "Skies", "Trees", "Inventory", "Horses", "Magic", "Quests", "Dungeons", "UI",
];
const SUFFIX: &[&str] =
    &["", " SE", " Overhaul", " Patch", " Redone", " Reborn", " 4K", " Fixes", " Expanded", " Collection"];
const CATEGORY: &[&str] =
    &["Visuals", "Gameplay", "Audio", "Patches", "Characters", "Environment", "Interface", "Utilities"];
const AUTHOR: &[&str] = &[
    "arthmoor", "Kryptopyr", "Mator", "EnaiSiaion", "Ousnius", "powerofthree", "Mihail", "Elianora", "Kartoffels",
    "ParallaxGuy",
];
pub const MOD_COUNT: usize = 2000;

pub struct Mod {
    pub priority: usize,
    pub name: String,
    pub version: String,
    pub category: &'static str,
    pub size_mb: u32,
    pub enabled: bool,
    pub conflicts: u32,
    pub author: &'static str,
    pub endorsements: u32,
}

pub fn generate() -> Vec<Mod> {
    let mut state: u32 = 0x9e37_79b9;
    let mut next = move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        state
    };
    let mut mods = Vec::with_capacity(MOD_COUNT);
    for priority in 0..MOD_COUNT {
        let prefix = PREFIX[next() as usize % PREFIX.len()];
        let subject = SUBJECT[next() as usize % SUBJECT.len()];
        let suffix = SUFFIX[next() as usize % SUFFIX.len()];
        let (major, minor, patch) = (1 + next() % 4, next() % 10, next() % 20);
        let category = CATEGORY[next() as usize % CATEGORY.len()];
        let size_mb = 1 + next() % 4000;
        let enabled = next() % 10 < 7;
        let conflicts = if next() % 100 < 12 { 1 + next() % 40 } else { 0 };
        let author = AUTHOR[next() as usize % AUTHOR.len()];
        let endorsements = next() % 50_000;
        mods.push(Mod {
            priority,
            name: format!("{prefix} {subject}{suffix}"),
            version: format!("{major}.{minor}.{patch}"),
            category,
            size_mb,
            enabled,
            conflicts,
            author,
            endorsements,
        });
    }
    mods
}

pub fn fmt_size(mb: u32) -> String {
    if mb >= 1024 { format!("{:.1} GB", f64::from(mb) / 1024.0) } else { format!("{mb} MB") }
}

/// `1234567` → `1,234,567` (like JS `toLocaleString("en-US")`).
pub fn fmt_int(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Warn,
    Danger,
}

pub fn status_of(m: &Mod) -> (Tone, String) {
    match m.conflicts {
        0 => (Tone::Ok, "OK".into()),
        n => (if n > 25 { Tone::Danger } else { Tone::Warn }, format!("{n} conflicts")),
    }
}

pub struct NxmLink {
    pub game: String,
    pub module: String,
    pub file: String,
    pub expires: String,
    pub source: &'static str,
}

/// `nxm://<game>/mods/<mod>/files/<file>?key=…&expires=…`
pub fn parse_nxm(url: &str, source: &'static str) -> NxmLink {
    let invalid = || NxmLink { game: "invalid".into(), module: "–".into(), file: "–".into(), expires: "–".into(), source };
    let Some(rest) = url.trim().strip_prefix("nxm://").or_else(|| url.trim().strip_prefix("NXM://")) else {
        return invalid();
    };
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let parts: Vec<&str> = path.split('/').collect();
    let [game, "mods", module, "files", file] = parts.as_slice() else { return invalid() };
    if !module.chars().all(|c| c.is_ascii_digit()) || !file.chars().all(|c| c.is_ascii_digit()) {
        return invalid();
    }
    let expires = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("expires="))
        .and_then(|v| v.parse::<i64>().ok())
        .map_or_else(|| "–".to_owned(), utc_minutes);
    NxmLink { game: (*game).into(), module: (*module).into(), file: (*file).into(), expires, source }
}

/// Unix seconds → `YYYY-MM-DD HH:MM` (UTC), like the web UI's ISO slice.
fn utc_minutes(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3600, rem % 3600 / 60)
}
