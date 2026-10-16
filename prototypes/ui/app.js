// Loom prototype UI — identical for the Electron and Tauri shells. Everything
// shell-specific goes through `host` (window.loom from Electron's preload, or
// the Tauri adapter below). The egui shell reimplements this file in Rust
// with the same data generator, layout and behavior.
"use strict";

// ------------------------------------------------------------------ host
function tauriHost() {
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;
  return {
    info: () => invoke("info"),
    metrics: () => invoke("metrics"),
    startupMs: () => invoke("startup_ms"),
    ready: () => invoke("ready"),
    benchScenario: () => invoke("bench_scenario"),
    takeNxm: () => invoke("take_nxm"),
    onNxm: (cb) => listen("nxm", (e) => cb(e.payload)),
    registerNxm: () => invoke("register_nxm"),
    openExternal: (url) => invoke("open_external", { url }),
    browser: {
      supported: true,
      show: (r) => invoke("browser_show", r),
      hide: () => invoke("browser_hide"),
      navigate: (url) => invoke("browser_navigate", { url }),
      back: () => invoke("browser_back"),
      forward: () => invoke("browser_forward"),
      reload: () => invoke("browser_reload"),
      onNavigate: (cb) => listen("browser-nav", (e) => cb(e.payload)),
    },
  };
}
const host = window.loom || (window.__TAURI__ ? tauriHost() : null);

// ------------------------------------------------------------------ data
// Deterministic generator shared with egui/src/data.rs — same LCG, same
// lists, same call order, so all three shells show the same 2000 mods.
const PREFIX = ["Immersive", "Enhanced", "Unofficial", "Realistic", "Better", "Ultimate", "Simple", "Dynamic", "Legacy of", "Vanilla Plus", "HD", "Lore-Friendly", "Interesting", "Classic", "Static Mesh"];
const SUBJECT = ["Armor", "Weapons", "Lighting", "Water", "Landscapes", "Cities", "Followers", "Combat", "Dragons", "Interiors", "Sounds", "Textures", "Skies", "Trees", "Inventory", "Horses", "Magic", "Quests", "Dungeons", "UI"];
const SUFFIX = ["", " SE", " Overhaul", " Patch", " Redone", " Reborn", " 4K", " Fixes", " Expanded", " Collection"];
const CATEGORY = ["Visuals", "Gameplay", "Audio", "Patches", "Characters", "Environment", "Interface", "Utilities"];
const AUTHOR = ["arthmoor", "Kryptopyr", "Mator", "EnaiSiaion", "Ousnius", "powerofthree", "Mihail", "Elianora", "Kartoffels", "ParallaxGuy"];
const MOD_COUNT = 2000;

function generateMods() {
  let state = 0x9e3779b9;
  const next = () => (state = (Math.imul(state, 1664525) + 1013904223) >>> 0);
  const pick = (a) => a[next() % a.length];
  const mods = [];
  for (let i = 0; i < MOD_COUNT; i++) {
    const name = `${pick(PREFIX)} ${pick(SUBJECT)}${pick(SUFFIX)}`;
    const major = 1 + (next() % 4), minor = next() % 10, patch = next() % 20;
    const category = pick(CATEGORY);
    const sizeMb = 1 + (next() % 4000);
    const enabled = next() % 10 < 7;
    const conflicts = next() % 100 < 12 ? 1 + (next() % 40) : 0;
    const author = pick(AUTHOR);
    const endorsements = next() % 50000;
    mods.push({ priority: i, name, version: `${major}.${minor}.${patch}`, category, sizeMb, enabled, conflicts, author, endorsements });
  }
  return mods;
}

const fmtSize = (mb) => (mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`);
const fmtInt = (n) => n.toLocaleString("en-US");
const statusOf = (m) => (m.conflicts === 0 ? ["ok", "OK"] : [m.conflicts > 25 ? "danger" : "warn", `${m.conflicts} conflicts`]);

// ------------------------------------------------------------------ state
const mods = generateMods();
let filter = "all";
let query = "";
let visible = mods;
let selected = 0;
const $ = (id) => document.getElementById(id);

// ------------------------------------------------------------------ navigation
const TITLES = { mods: "Mods", downloads: "Downloads", nexus: "Nexus Mods", media: "Media", perf: "Performance" };
let page = "mods";
function showPage(name) {
  page = name;
  for (const b of document.querySelectorAll("#nav button")) b.classList.toggle("active", b.dataset.page === name);
  for (const s of document.querySelectorAll(".page")) s.classList.toggle("active", s.dataset.page === name);
  $("title").textContent = TITLES[name];
  $("search").style.visibility = name === "mods" ? "visible" : "hidden";
  updateSubtitle();
  if (name === "nexus") placeBrowser(); else if (host && host.browser.supported) host.browser.hide();
  if (name === "mods") renderRows();
  if (name === "perf") drawSparks();
}
for (const b of document.querySelectorAll("#nav button")) b.onclick = () => showPage(b.dataset.page);

function updateSubtitle() {
  const s = page === "mods" ? `${fmtInt(visible.length)} of ${fmtInt(mods.length)}` : "";
  $("subtitle").textContent = s;
}

// ------------------------------------------------------------------ mods list
const ROW = 36;
const viewport = $("viewport");
const spacer = $("spacer");
const pool = new Map();

function applyFilter() {
  const q = query.toLowerCase();
  visible = mods.filter((m) =>
    (filter === "all" || (filter === "enabled" && m.enabled) || (filter === "disabled" && !m.enabled) || (filter === "conflicts" && m.conflicts > 0)) &&
    (!q || m.name.toLowerCase().includes(q)));
  spacer.style.height = `${visible.length * ROW}px`;
  for (const el of pool.values()) el.remove();
  pool.clear();
  updateSubtitle();
  renderRows();
}

function rowEl(m, idx) {
  const el = document.createElement("div");
  el.className = "row" + (m.priority === selected ? " sel" : "");
  el.style.top = `${idx * ROW}px`;
  const [cls, label] = statusOf(m);
  el.innerHTML =
    `<span><div class="toggle${m.enabled ? " on" : ""}"></div></span>` +
    `<span class="dim">${m.priority}</span>` +
    `<span class="name"></span>` +
    `<span class="dim">${m.version}</span>` +
    `<span class="dim">${m.category}</span>` +
    `<span class="dim">${fmtSize(m.sizeMb)}</span>` +
    `<span><span class="badge ${cls}">${label}</span></span>`;
  el.querySelector(".name").textContent = m.name;
  el.querySelector(".toggle").onclick = (e) => { e.stopPropagation(); m.enabled = !m.enabled; refreshRow(m); updateEnabled(); renderDetails(); };
  el.onclick = () => { selected = m.priority; for (const r of pool.values()) r.classList.remove("sel"); el.classList.add("sel"); renderDetails(); };
  return el;
}

function refreshRow(m) {
  const idx = visible.indexOf(m);
  const old = pool.get(idx);
  if (old) { const fresh = rowEl(m, idx); old.replaceWith(fresh); pool.set(idx, fresh); }
}

function renderRows() {
  if (page !== "mods") return;
  const first = Math.max(0, Math.floor(viewport.scrollTop / ROW) - 8);
  const last = Math.min(visible.length, Math.ceil((viewport.scrollTop + viewport.clientHeight) / ROW) + 8);
  for (const [idx, el] of pool) if (idx < first || idx >= last) { el.remove(); pool.delete(idx); }
  for (let i = first; i < last; i++) if (!pool.has(i)) { const el = rowEl(visible[i], i); viewport.appendChild(el); pool.set(i, el); }
}
viewport.addEventListener("scroll", renderRows, { passive: true });
window.addEventListener("resize", () => { renderRows(); if (page === "nexus") placeBrowser(); drawSparks(); });

for (const c of document.querySelectorAll(".chip")) c.onclick = () => {
  filter = c.dataset.filter;
  for (const o of document.querySelectorAll(".chip")) o.classList.toggle("active", o === c);
  applyFilter();
};
$("search").oninput = (e) => { query = e.target.value; applyFilter(); };

function renderDetails() {
  const m = mods[selected];
  const [cls, label] = statusOf(m);
  $("details").innerHTML = `
    <h2></h2>
    <div class="meta">by ${m.author} · v${m.version}</div>
    <div class="card"><div class="kv">
      <span class="k">Priority</span><span>${m.priority}</span>
      <span class="k">Category</span><span>${m.category}</span>
      <span class="k">Size</span><span>${fmtSize(m.sizeMb)}</span>
      <span class="k">Endorsements</span><span>${fmtInt(m.endorsements)}</span>
      <span class="k">Status</span><span><span class="badge ${cls}">${label}</span></span>
      <span class="k">State</span><span>${m.enabled ? "Enabled" : "Disabled"}</span>
    </div></div>
    <div class="card">
      <h3>Actions</h3>
      <div class="row-actions" style="margin:0">
        <button class="btn" id="d-toggle">${m.enabled ? "Disable" : "Enable"}</button>
        <button class="btn" id="d-nexus"><i class="fi">&#xE774;</i>Find on Nexus</button>
      </div>
    </div>`;
  $("details").querySelector("h2").textContent = m.name;
  $("d-toggle").onclick = () => { m.enabled = !m.enabled; refreshRow(m); updateEnabled(); renderDetails(); };
  $("d-nexus").onclick = () => {
    $("b-url").value = `https://www.nexusmods.com/games/skyrimspecialedition/mods?keyword=${encodeURIComponent(m.name)}`;
    showPage("nexus"); navigate();
  };
}

function updateEnabled() {
  const n = mods.filter((m) => m.enabled).length;
  $("st-enabled").textContent = `${fmtInt(n)} of ${fmtInt(mods.length)} enabled`;
  $("nav-mods").textContent = fmtInt(mods.length);
}

// ------------------------------------------------------------------ nxm downloads
const links = [];
function parseNxm(url) {
  // nxm://<game>/mods/<mod>/files/<file>?key=…&expires=…&user_id=…
  const m = /^nxm:\/\/([^/]+)\/mods\/(\d+)\/files\/(\d+)(?:\?(.*))?$/i.exec(url.trim());
  if (!m) return null;
  const q = new URLSearchParams(m[4] || "");
  const exp = Number(q.get("expires"));
  return { game: m[1], mod: m[2], file: m[3], expires: exp ? new Date(exp * 1000).toISOString().replace("T", " ").slice(0, 16) : "–" };
}
function addLink(url, source) {
  const p = parseNxm(url);
  links.unshift(p ? { ...p, source } : { game: "invalid", mod: "–", file: "–", expires: "–", source });
  const tbody = $("nxm-rows");
  tbody.innerHTML = "";
  for (const l of links) {
    const tr = document.createElement("tr");
    for (const v of [l.game, l.mod, l.file, l.expires, l.source]) { const td = document.createElement("td"); td.textContent = v; tr.appendChild(td); }
    tr.children[0].className = "mono";
    tbody.appendChild(tr);
  }
  $("nxm-empty").style.display = links.length ? "none" : "block";
  $("nav-dl").textContent = links.length;
}
$("nxm-add").onclick = () => { if ($("nxm-input").value) { addLink($("nxm-input").value, "pasted"); $("nxm-input").value = ""; } };
$("nxm-sim").onclick = () => {
  const mod = 266 + links.length;
  addLink(`nxm://skyrimspecialedition/mods/${mod}/files/${100000 + mod}?key=demo&expires=${Math.floor(Date.now() / 1000) + 3600}&user_id=1`, "simulated");
};
$("nxm-reg").onclick = async () => {
  if (!host) return;
  const r = await host.registerNxm();
  $("nxm-status").textContent = r.message;
};

// ------------------------------------------------------------------ embedded browser
function placeBrowser() {
  if (!host || !host.browser.supported) {
    $("web-placeholder").innerHTML = "<div><b>No embedded browser in this shell</b>Use “Open in browser”; nxm:// links still arrive in Downloads.</div>";
    return;
  }
  const r = $("webhost").getBoundingClientRect();
  host.browser.show({ x: Math.round(r.left), y: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) });
}
function navigate() {
  let url = $("b-url").value.trim();
  if (!/^[a-z][a-z0-9+.-]*:/i.test(url)) url = "https://" + url; // bare host → https
  if (host && host.browser.supported) host.browser.navigate(url);
}
$("b-url").onkeydown = (e) => { if (e.key === "Enter") navigate(); };
$("b-back").onclick = () => host && host.browser.back();
$("b-fwd").onclick = () => host && host.browser.forward();
$("b-reload").onclick = () => host && host.browser.reload();
$("b-ext").onclick = () => host && host.openExternal($("b-url").value);

// ------------------------------------------------------------------ media
function setupMedia() {
  $("video-slot").innerHTML = '<video id="video" src="media/sample.mp4" controls preload="metadata" loop></video>';
  const v = document.createElement("video");
  const codecs = [
    ["H.264 / AVC", 'video/mp4; codecs="avc1.42E01E"'],
    ["H.265 / HEVC", 'video/mp4; codecs="hvc1.1.6.L93.B0"'],
    ["VP9", 'video/webm; codecs="vp9"'],
    ["AV1", 'video/mp4; codecs="av01.0.05M.08"'],
    ["AAC", 'audio/mp4; codecs="mp4a.40.2"'],
    ["Opus", 'audio/webm; codecs="opus"'],
  ];
  $("codecs").innerHTML = codecs.map(([n, t]) => {
    const s = v.canPlayType(t);
    const [cls, label] = s === "probably" ? ["ok", "Yes"] : s === "maybe" ? ["warn", "Maybe"] : ["danger", "No"];
    return `<tr><td>${n}</td><td><span class="badge ${cls}">${label}</span></td></tr>`;
  }).join("");
  $("yt").onclick = () => { $("b-url").value = "https://www.youtube.com/results?search_query=skyrim+mod+showcase"; showPage("nexus"); navigate(); };
}

// ------------------------------------------------------------------ performance
const memHist = [], cpuHist = [];
function drawSpark(canvas, data, color, max) {
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth, h = canvas.clientHeight;
  if (!w || !h) return;
  canvas.width = w * dpr; canvas.height = h * dpr;
  const g = canvas.getContext("2d");
  g.scale(dpr, dpr);
  g.strokeStyle = "#272e3a"; g.lineWidth = 1;
  for (let i = 1; i < 4; i++) { const y = Math.round((h * i) / 4) + 0.5; g.beginPath(); g.moveTo(0, y); g.lineTo(w, y); g.stroke(); }
  if (data.length < 2) return;
  const top = Math.max(max, ...data) * 1.15 || 1;
  const pts = data.map((v, i) => [(i / 59) * w, h - (v / top) * h]);
  const off = w - pts[pts.length - 1][0];
  g.beginPath(); pts.forEach(([x, y], i) => (i ? g.lineTo(x + off, y) : g.moveTo(x + off, y)));
  g.lineTo(w, h); g.lineTo(pts[0][0] + off, h); g.closePath();
  g.fillStyle = color + "22"; g.fill();
  g.beginPath(); pts.forEach(([x, y], i) => (i ? g.lineTo(x + off, y) : g.moveTo(x + off, y)));
  g.strokeStyle = color; g.lineWidth = 2; g.stroke();
}
function drawSparks() {
  if (page !== "perf") return;
  drawSpark($("spark-mem"), memHist, "#8b6cff", 100);
  drawSpark($("spark-cpu"), cpuHist, "#2dd4bf", 10);
}
async function pollMetrics() {
  if (!host) return;
  const m = await host.metrics();
  memHist.push(m.workingSetMb); cpuHist.push(m.cpuPercent);
  if (memHist.length > 60) { memHist.shift(); cpuHist.shift(); }
  $("p-mem").innerHTML = `${m.workingSetMb.toFixed(0)}<span class="u">MB</span>`;
  $("p-cpu").innerHTML = `${m.cpuPercent.toFixed(1)}<span class="u">%</span>`;
  $("p-procs").textContent = m.processes;
  $("st-mem").textContent = `${m.workingSetMb.toFixed(0)} MB`;
  $("st-cpu").textContent = `${m.cpuPercent.toFixed(1)}%`;
  drawSparks();
}

function scrollStress(seconds) {
  showPage("mods");
  const end = performance.now() + seconds * 1000;
  let dir = 1;
  const step = () => {
    viewport.scrollTop += dir * 48;
    if (viewport.scrollTop + viewport.clientHeight >= viewport.scrollHeight - 1) dir = -1;
    if (viewport.scrollTop <= 0) dir = 1;
    if (performance.now() < end) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}
function playVideo() { showPage("media"); const v = $("video"); v.muted = true; v.play(); }
$("sc-scroll").onclick = () => scrollStress(10);
$("sc-video").onclick = playVideo;
$("launch").onclick = () => {};

// ------------------------------------------------------------------ boot
async function boot() {
  applyFilter();
  renderDetails();
  updateEnabled();
  setupMedia();
  if (!host) return;
  const info = await host.info();
  $("shell-sub").textContent = `${info.shell} prototype`;
  $("st-shell").textContent = `${info.shell} ${info.version}`;
  $("shell-kv").innerHTML = [["Shell", `${info.shell} ${info.version}`], ["Engine", info.engine], ["Renderer", info.renderer]]
    .map(([k, v]) => `<span class="k">${k}</span><span>${v}</span>`).join("");
  host.onNxm((url) => { addLink(url, "nxm handler"); showPage("downloads"); });
  if (host.browser.supported) host.browser.onNavigate((url) => { $("b-url").value = url; $("web-placeholder").style.display = "none"; });
  for (const url of await host.takeNxm()) addLink(url, "nxm handler");
  requestAnimationFrame(() => requestAnimationFrame(async () => {
    await host.ready();
    const ms = await host.startupMs();
    $("p-start").innerHTML = `${ms.toFixed(0)}<span class="u">ms</span>`;
    const scenario = await host.benchScenario();
    if (scenario === "scroll") scrollStress(1e9);
    if (scenario === "video") playVideo();
    if (scenario && scenario.startsWith("page:")) showPage(scenario.slice(5));
    if (scenario && scenario.startsWith("url:")) { $("b-url").value = scenario.slice(4); showPage("nexus"); navigate(); }
  }));
  pollMetrics();
  setInterval(pollMetrics, 1000);
  if (links.length) showPage("downloads");
}
boot();
