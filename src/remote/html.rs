pub const REMOTE_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, user-scalable=no">
<title>Ostendo Remote</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600;700&display=swap" rel="stylesheet">
<style>
  :root {
    --bg: #1a1614;
    --bg-surface: #241f1c;
    --bg-elevated: #2a2420;
    --bg-hover: #2e2825;
    --border: #3d3530;
    --border-subtle: #322c28;
    --text: #e8e0d8;
    --text-secondary: #a09080;
    --text-tertiary: #6b5f55;
    --accent: #c97b4b;
    --accent-hover: #d4895a;
    --accent-subtle: rgba(201,123,75,0.12);
    --accent-glow: rgba(201,123,75,0.06);
    --green: #6b9e6e;
    --green-subtle: rgba(107,158,110,0.12);
    --red: #c75f5f;
    --red-subtle: rgba(199,95,95,0.12);
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    background: var(--bg);
    color: var(--text);
    font-family: 'JetBrains Mono', monospace;
    display: flex; flex-direction: column;
    height: 100vh; height: 100dvh;
    -webkit-user-select: none; user-select: none;
    overflow: hidden;
  }

  /* ── Header ── */
  header {
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border);
    padding: 12px 16px;
    display: flex; align-items: center; justify-content: space-between;
    flex-shrink: 0;
    position: relative;
  }
  .logo {
    font-size: 0.85rem; font-weight: 700;
    color: var(--accent);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .header-right {
    display: flex; align-items: center; gap: 12px;
  }
  #timer {
    color: var(--text-secondary);
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }
  .conn-dot {
    width: 7px; height: 7px;
    border-radius: 50%;
    background: var(--text-tertiary);
    transition: background 0.3s;
  }
  .conn-dot.live { background: var(--green); box-shadow: 0 0 6px rgba(107,158,110,0.5); }

  /* ── Progress bar ── */
  #progress {
    position: absolute; bottom: 0; left: 0;
    height: 2px;
    background: var(--accent);
    transition: width 0.3s ease;
    border-radius: 0 1px 0 0;
  }

  /* ── Main two-column ── */
  main {
    display: flex; flex: 1;
    overflow: hidden;
    min-height: 0;
  }

  /* ── Slide panel (left) ── */
  #slide-panel {
    flex: 3; display: flex; flex-direction: column;
    border-right: 1px solid var(--border-subtle);
    min-width: 0;
  }
  #slide-header {
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    display: flex; align-items: flex-start; justify-content: space-between;
  }
  .slide-meta {
    display: flex; align-items: center; gap: 8px;
    margin-bottom: 6px;
  }
  #slide-counter {
    color: var(--accent);
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.04em;
  }
  #section-badge {
    font-size: 0.65rem;
    padding: 2px 8px;
    border-radius: 10px;
    background: var(--accent-subtle);
    color: var(--accent);
    font-weight: 500;
    display: none;
  }
  #section-badge.visible { display: inline-block; }
  #slide-title {
    font-size: 1rem;
    font-weight: 600;
    line-height: 1.4;
    color: var(--text);
  }
  #slide-content {
    flex: 1; overflow-y: auto;
    padding: 12px 16px;
    font-size: 0.78rem;
    line-height: 1.65;
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
  }
  #slide-content::-webkit-scrollbar { width: 4px; }
  #slide-content::-webkit-scrollbar-thumb { background: var(--border); border-radius: 2px; }

  /* ── Notes panel (right) ── */
  #notes-panel {
    flex: 2; display: flex; flex-direction: column;
    background: var(--bg-elevated);
    min-width: 0;
  }
  #notes-header {
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    display: flex; align-items: center; justify-content: space-between;
  }
  #notes-label {
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-tertiary);
  }
  #notes-body {
    flex: 1; overflow-y: auto;
    padding: 12px 16px;
    font-size: 0.8rem;
    line-height: 1.7;
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
  }
  #notes-body::-webkit-scrollbar { width: 4px; }
  #notes-body::-webkit-scrollbar-thumb { background: var(--border); border-radius: 2px; }
  .notes-empty {
    color: var(--text-tertiary);
    font-style: italic;
  }

  /* ── Local font size buttons ── */
  .font-sizer {
    display: flex; align-items: center; gap: 2px;
    flex-shrink: 0;
  }
  .font-sizer-btn {
    width: 24px; height: 24px;
    display: flex; align-items: center; justify-content: center;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-tertiary);
    font-family: inherit;
    font-size: 0.6rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
    padding: 0;
    line-height: 1;
  }
  .font-sizer-btn:hover { color: var(--text-secondary); border-color: var(--accent); }
  .font-sizer-btn:active { background: var(--bg-hover); }

  /* ── Controls area ── */
  #controls {
    background: var(--bg-surface);
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }

  /* Navigation row */
  #nav-row {
    display: flex; gap: 8px;
    padding: 10px 12px;
  }
  .nav-btn {
    flex: 1;
    padding: 14px 0;
    border: none;
    border-radius: 8px;
    font-family: inherit;
    font-size: 0.85rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
    letter-spacing: 0.02em;
  }
  .nav-prev {
    background: var(--bg-hover);
    color: var(--text);
    border: 1px solid var(--border);
  }
  .nav-prev:active { background: var(--border); }
  .nav-next {
    background: var(--accent);
    color: var(--bg);
  }
  .nav-next:active { background: var(--accent-hover); }

  /* Jump row */
  #jump-row {
    display: flex; align-items: center; gap: 8px;
    padding: 0 12px 8px;
  }
  #jump-input {
    width: 64px;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: 6px;
    padding: 6px 8px;
    font-family: inherit;
    font-size: 0.78rem;
    text-align: center;
    outline: none;
    transition: border-color 0.15s;
  }
  #jump-input:focus { border-color: var(--accent); }
  #jump-btn {
    background: var(--bg-hover);
    color: var(--text-secondary);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 14px;
    font-family: inherit;
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }
  #jump-btn:hover { color: var(--text); border-color: var(--accent); }
  #theme-select {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text-secondary);
    border-radius: 6px;
    padding: 5px 8px;
    font-family: inherit;
    font-size: 0.68rem;
    font-weight: 500;
    cursor: pointer;
    outline: none;
    transition: border-color 0.15s;
    max-width: 140px;
  }
  #theme-select:focus { border-color: var(--accent); }
  #theme-select option { background: var(--bg-surface); color: var(--text); }

  /* Expand button */
  #expand-btn {
    display: block;
    width: 100%;
    background: none;
    border: none;
    border-top: 1px solid var(--border-subtle);
    color: var(--text-tertiary);
    font-family: inherit;
    font-size: 0.7rem;
    font-weight: 500;
    padding: 7px 0;
    cursor: pointer;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    transition: color 0.15s;
  }
  #expand-btn:hover { color: var(--text-secondary); }

  /* Control panel (collapsible) */
  #ctrl-panel {
    max-height: 0;
    overflow: hidden;
    transition: max-height 0.25s ease;
    border-top: 1px solid var(--border-subtle);
  }
  #ctrl-panel.open { max-height: 500px; }
  .ctrl-group {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .ctrl-group:last-child { border-bottom: none; }
  .ctrl-group-label {
    font-size: 0.6rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-tertiary);
    margin-bottom: 6px;
  }
  .ctrl-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(80px, 1fr));
    gap: 6px;
  }
  .ctrl-btn {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    padding: 8px 4px;
    min-height: 44px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text-secondary);
    font-family: inherit;
    font-size: 0.65rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    line-height: 1.3;
    text-align: center;
    gap: 2px;
  }
  .ctrl-btn:hover { background: var(--bg-hover); color: var(--text); }
  .ctrl-btn:active { background: var(--border); }
  .ctrl-btn.active {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }
  .ctrl-btn.green { border-color: var(--green); color: var(--green); }
  .ctrl-btn.green:hover { background: var(--green-subtle); }
  .ctrl-btn.red { color: var(--red); }
  .ctrl-btn.red:hover { background: var(--red-subtle); }
  .ctrl-btn .icon { font-size: 1rem; line-height: 1; }
  .ctrl-btn .badge {
    font-size: 0.55rem;
    color: var(--text-tertiary);
    font-weight: 400;
  }
  .ctrl-btn.hidden { display: none; }

  /* ── Status bar ── */
  #status {
    text-align: center;
    padding: 3px;
    font-size: 0.6rem;
    color: var(--text-tertiary);
    background: var(--bg);
    letter-spacing: 0.04em;
  }

  /* ── Mobile responsive ── */
  @media (max-width: 640px) {
    main { flex-direction: column; }
    #slide-panel { border-right: none; border-bottom: 1px solid var(--border-subtle); flex: 1; }
    #notes-panel { flex: none; max-height: 30vh; min-height: 80px; }
    .ctrl-grid { grid-template-columns: repeat(auto-fill, minmax(72px, 1fr)); }
  }
</style>
</head>
<body>

<header>
  <span class="logo">Ostendo</span>
  <div class="header-right">
    <span id="timer"></span>
    <span class="conn-dot" id="conn-dot"></span>
  </div>
  <div id="progress" style="width:0%"></div>
</header>

<main>
  <section id="slide-panel">
    <div id="slide-header">
      <div>
        <div class="slide-meta">
          <span id="slide-counter">-- / --</span>
          <span id="section-badge"></span>
        </div>
        <div id="slide-title">Connecting...</div>
      </div>
      <div class="font-sizer">
        <button class="font-sizer-btn" onclick="adjustLocalFont('slide',-1)">A&#8722;</button>
        <button class="font-sizer-btn" onclick="adjustLocalFont('slide',1)">A&#43;</button>
      </div>
    </div>
    <div id="slide-content"></div>
  </section>

  <section id="notes-panel">
    <div id="notes-header">
      <span id="notes-label">Speaker Notes</span>
      <div class="font-sizer">
        <button class="font-sizer-btn" onclick="adjustLocalFont('notes',-1)">A&#8722;</button>
        <button class="font-sizer-btn" onclick="adjustLocalFont('notes',1)">A&#43;</button>
      </div>
    </div>
    <div id="notes-body"><span class="notes-empty">No notes</span></div>
  </section>
</main>

<div id="controls">
  <div id="nav-row">
    <button class="nav-btn nav-prev" onclick="send('prev')">&#8592; Prev</button>
    <button class="nav-btn nav-next" onclick="send('next')">Next &#8594;</button>
  </div>
  <div id="jump-row">
    <input type="number" id="jump-input" min="1" placeholder="#">
    <button id="jump-btn" onclick="jumpToSlide()">Go to slide</button>
    <span style="flex:1"></span>
    <select id="theme-select" onchange="setTheme(this.value)"></select>
  </div>
  <button id="expand-btn" onclick="togglePanel()">&#9662; Controls</button>
  <div id="ctrl-panel">

    <div class="ctrl-group">
      <div class="ctrl-group-label">Navigation</div>
      <div class="ctrl-grid">
        <button class="ctrl-btn" onclick="send('prev_section')"><span class="icon">&#171;</span>Section</button>
        <button class="ctrl-btn" onclick="send('next_section')"><span class="icon">&#187;</span>Section</button>
        <button class="ctrl-btn" onclick="send('scroll_up')"><span class="icon">&#8593;</span>Scroll</button>
        <button class="ctrl-btn" onclick="send('scroll_down')"><span class="icon">&#8595;</span>Scroll</button>
      </div>
    </div>

    <div class="ctrl-group">
      <div class="ctrl-group-label">Display</div>
      <div class="ctrl-grid">
        <button class="ctrl-btn" id="btn-fullscreen" onclick="send('toggle_fullscreen')"><span class="icon">&#9634;</span>Fullscreen</button>
        <button class="ctrl-btn" id="btn-notes" onclick="send('toggle_notes')"><span class="icon">&#9776;</span>Notes</button>
        <button class="ctrl-btn" id="btn-theme-name" onclick="send('toggle_theme_name')"><span class="icon">T</span>Theme</button>
        <button class="ctrl-btn" id="btn-sections" onclick="send('toggle_sections')"><span class="icon">&#167;</span>Sections</button>
        <button class="ctrl-btn" id="btn-dark-mode" onclick="send('toggle_dark_mode')"><span class="icon">&#9788;</span>Light/Dark</button>
      </div>
    </div>

    <div class="ctrl-group">
      <div class="ctrl-group-label">Scale</div>
      <div class="ctrl-grid">
        <button class="ctrl-btn" onclick="send('scale_down')"><span class="icon">&#8722;</span>Content<span class="badge" id="scale-val"></span></button>
        <button class="ctrl-btn" onclick="send('scale_up')"><span class="icon">&#43;</span>Content</button>
        <button class="ctrl-btn" onclick="send('image_scale_down')"><span class="icon">&#8722;</span>Image<span class="badge" id="img-scale-val"></span></button>
        <button class="ctrl-btn" onclick="send('image_scale_up')"><span class="icon">&#43;</span>Image</button>
        <button class="ctrl-btn" onclick="send('font_down')"><span class="icon">A&#8722;</span>Font<span class="badge" id="font-val"></span></button>
        <button class="ctrl-btn" onclick="send('font_up')"><span class="icon">A&#43;</span>Font</button>
        <button class="ctrl-btn" onclick="send('font_reset')"><span class="icon">A</span>Reset</button>
      </div>
    </div>

    <div class="ctrl-group">
      <div class="ctrl-group-label">Actions</div>
      <div class="ctrl-grid">
        <button class="ctrl-btn green hidden" id="btn-exec" onclick="send('execute_code')"><span class="icon">&#9654;</span>Run Code</button>
        <button class="ctrl-btn" id="btn-timer" onclick="send('timer_start')"><span class="icon">&#9201;</span>Timer</button>
        <button class="ctrl-btn red" onclick="send('timer_reset')"><span class="icon">&#8634;</span>Reset</button>
      </div>
    </div>

  </div>
</div>

<div id="status">disconnected</div>

<script>
let ws, state = {}, touchStartX = 0, panelOpen = false;
var localFontSizes = { slide: 0.78, notes: 0.8 };

function connect() {
  var params = new URLSearchParams(window.location.hash.slice(1));
  var token = params.get('token');
  var wsUrl = "ws://" + location.host;
  ws = token ? new WebSocket(wsUrl, token) : new WebSocket(wsUrl);
  ws.onopen = function() {
    document.getElementById("status").textContent = "connected";
    document.getElementById("conn-dot").classList.add("live");
  };
  ws.onmessage = function(e) {
    var d = JSON.parse(e.data);
    if (d.type === "state") { state = d; updateUI(d); }
  };
  ws.onclose = function() {
    document.getElementById("status").textContent = "reconnecting\u2026";
    document.getElementById("conn-dot").classList.remove("live");
    setTimeout(connect, 2000);
  };
  ws.onerror = function() { ws.close(); };
}

function send(action, extra) {
  if (ws && ws.readyState === 1) {
    var msg = {type:"command", action:action};
    if (extra !== undefined) {
      if (typeof extra === "number") msg.slide = extra;
      else if (typeof extra === "string") msg.theme = extra;
    }
    ws.send(JSON.stringify(msg));
  }
}

function setTheme(slug) {
  if (slug) send("set_theme", slug);
}

function applyThemeColors(bg, accent, text) {
  var r = document.documentElement.style;
  // Parse hex to RGB
  function hexRgb(h) {
    h = h.replace("#","");
    return [parseInt(h.substring(0,2),16), parseInt(h.substring(2,4),16), parseInt(h.substring(4,6),16)];
  }
  function rgbHex(rgb) {
    return "#" + rgb.map(function(c){ return Math.max(0,Math.min(255,Math.round(c))).toString(16).padStart(2,"0"); }).join("");
  }
  function lum(rgb) {
    var c = rgb.map(function(v){ v=v/255; return v<=0.03928?v/12.92:Math.pow((v+0.055)/1.055,2.4); });
    return 0.2126*c[0]+0.7152*c[1]+0.0722*c[2];
  }
  function mix(a,b,t) { return a.map(function(v,i){ return v+(b[i]-v)*t; }); }

  var bgRgb = hexRgb(bg), acRgb = hexRgb(accent), txRgb = hexRgb(text);
  var bgLum = lum(bgRgb);
  var isDark = bgLum < 0.15;

  // Derive surface/elevated/hover by shifting bg toward text slightly
  var surface = rgbHex(mix(bgRgb, isDark ? [255,255,255] : [0,0,0], isDark ? 0.06 : 0.04));
  var elevated = rgbHex(mix(bgRgb, isDark ? [255,255,255] : [0,0,0], isDark ? 0.09 : 0.06));
  var hover = rgbHex(mix(bgRgb, isDark ? [255,255,255] : [0,0,0], isDark ? 0.12 : 0.08));
  var border = rgbHex(mix(bgRgb, isDark ? [255,255,255] : [0,0,0], isDark ? 0.18 : 0.14));
  var borderSubtle = rgbHex(mix(bgRgb, isDark ? [255,255,255] : [0,0,0], isDark ? 0.13 : 0.10));
  var textSec = rgbHex(mix(txRgb, bgRgb, 0.35));
  var textTer = rgbHex(mix(txRgb, bgRgb, 0.60));
  var accentHover = rgbHex(mix(acRgb, isDark ? [255,255,255] : [0,0,0], 0.15));

  r.setProperty("--bg", bg);
  r.setProperty("--bg-surface", surface);
  r.setProperty("--bg-elevated", elevated);
  r.setProperty("--bg-hover", hover);
  r.setProperty("--border", border);
  r.setProperty("--border-subtle", borderSubtle);
  r.setProperty("--text", text);
  r.setProperty("--text-secondary", textSec);
  r.setProperty("--text-tertiary", textTer);
  r.setProperty("--accent", accent);
  r.setProperty("--accent-hover", accentHover);
  r.setProperty("--accent-subtle", "rgba("+acRgb[0]+","+acRgb[1]+","+acRgb[2]+",0.12)");

  // Nav next button text: use bg or text depending on accent luminance
  var navNext = document.querySelector(".nav-next");
  if (navNext) navNext.style.color = lum(acRgb) > 0.4 ? bg : text;
}

function updateUI(d) {
  // Slide info
  document.getElementById("slide-counter").textContent = d.slide + " / " + d.total;
  document.getElementById("slide-title").textContent = d.slide_title || "Untitled";
  document.getElementById("progress").style.width = (d.total > 0 ? (d.slide / d.total * 100) : 0) + "%";

  // Section badge
  var badge = document.getElementById("section-badge");
  if (d.section) { badge.textContent = d.section; badge.classList.add("visible"); }
  else { badge.classList.remove("visible"); }

  // Timer
  document.getElementById("timer").textContent = d.timer || "";

  // Content preview (safe textContent, no HTML injection)
  var cp = document.getElementById("slide-content");
  if (d.slide_content && d.slide_content.length > 0) {
    cp.textContent = d.slide_content.join("\n");
  } else { cp.textContent = ""; }

  // Notes (safe textContent)
  var nb = document.getElementById("notes-body");
  if (d.notes) {
    nb.textContent = d.notes;
    nb.style.fontStyle = "normal";
    nb.style.color = "";
  } else {
    nb.textContent = "No notes for this slide";
    nb.style.fontStyle = "italic";
    nb.style.color = "var(--text-tertiary)";
  }

  // Jump input
  var ji = document.getElementById("jump-input");
  ji.max = d.total;
  ji.placeholder = d.slide;

  // Toggle states
  setToggle("btn-fullscreen", d.is_fullscreen);
  setToggle("btn-notes", d.is_notes_visible);
  setToggle("btn-theme-name", d.show_theme_name);
  setToggle("btn-sections", d.show_sections);
  setToggle("btn-dark-mode", !d.is_dark_mode);

  // Scale badges
  document.getElementById("scale-val").textContent = d.scale + "%";
  document.getElementById("img-scale-val").textContent = (d.image_scale >= 0 ? "+" : "") + d.image_scale;
  document.getElementById("font-val").textContent = (d.font_offset >= 0 ? "+" : "") + d.font_offset;

  // Theme selector
  var sel = document.getElementById("theme-select");
  if (d.themes && d.themes.length > 0 && sel.options.length !== d.themes.length) {
    sel.textContent = "";
    for (var i = 0; i < d.themes.length; i++) {
      var opt = document.createElement("option");
      opt.value = d.themes[i];
      opt.textContent = d.themes[i];
      sel.appendChild(opt);
    }
  }
  if (d.theme_slug) sel.value = d.theme_slug;

  // Dynamic theme colors
  if (d.theme_bg && d.theme_accent && d.theme_text) {
    applyThemeColors(d.theme_bg, d.theme_accent, d.theme_text);
  }

  // Execute button visibility
  var execBtn = document.getElementById("btn-exec");
  if (d.has_executable_code) { execBtn.classList.remove("hidden"); }
  else { execBtn.classList.add("hidden"); }

  // Timer button state
  var timerBtn = document.getElementById("btn-timer");
  if (d.timer_running) { timerBtn.classList.add("active"); }
  else { timerBtn.classList.remove("active"); }
}

function setToggle(id, active) {
  var el = document.getElementById(id);
  if (el) el.classList.toggle("active", !!active);
}

function jumpToSlide() {
  var v = parseInt(document.getElementById("jump-input").value);
  if (v > 0) send("goto", v);
  document.getElementById("jump-input").value = "";
}

function adjustLocalFont(target, delta) {
  var step = 0.04;
  var min = 0.5, max = 1.6;
  localFontSizes[target] = Math.min(max, Math.max(min, localFontSizes[target] + delta * step));
  var el = target === "slide" ? document.getElementById("slide-content") : document.getElementById("notes-body");
  el.style.fontSize = localFontSizes[target] + "rem";
}

function togglePanel() {
  panelOpen = !panelOpen;
  document.getElementById("ctrl-panel").classList.toggle("open", panelOpen);
  document.getElementById("expand-btn").textContent = panelOpen ? "\u25B4 Controls" : "\u25BE Controls";
}

document.addEventListener("keydown", function(e) {
  if (document.activeElement.id === "jump-input") {
    if (e.key === "Enter") jumpToSlide();
    return;
  }
  switch(e.key) {
    case "ArrowLeft": case "h": send("prev"); break;
    case "ArrowRight": case "l": case " ": send("next"); break;
    case "ArrowUp": case "k": send("scroll_up"); break;
    case "ArrowDown": case "j": send("scroll_down"); break;
    case "J": send("next_section"); break;
    case "K": send("prev_section"); break;
    case "f": send("toggle_fullscreen"); break;
    case "n": send("toggle_notes"); break;
    case "T": send("toggle_theme_name"); break;
    case "S": send("toggle_sections"); break;
    case "D": send("toggle_dark_mode"); break;
  }
});

// Touch/swipe
document.addEventListener("touchstart", function(e) { touchStartX = e.changedTouches[0].screenX; }, {passive:true});
document.addEventListener("touchend", function(e) {
  var dx = e.changedTouches[0].screenX - touchStartX;
  if (Math.abs(dx) > 50) { dx > 0 ? send("prev") : send("next"); }
}, {passive:true});

connect();
</script>
</body>
</html>"##;
