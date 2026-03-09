pub const REMOTE_HTML: &str = r##"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, user-scalable=no">
<title>Ostendo Remote</title>
<style>
  * { box-sizing: border-box; }
  body { background:#0d1117; color:#e6edf3; font-family:'SF Mono',Monaco,Consolas,monospace;
         display:flex; flex-direction:column; height:100vh; margin:0; padding:0;
         -webkit-user-select:none; user-select:none; }
  header { background:#161b22; border-bottom:1px solid #30363d; padding:12px 16px;
           display:flex; align-items:center; justify-content:space-between; }
  header h1 { font-size:1rem; margin:0; color:#58a6ff; }
  #timer { color:#8b949e; font-size:0.9rem; font-variant-numeric:tabular-nums; }
  #slide-info { background:#161b22; padding:12px 16px; border-bottom:1px solid #30363d; }
  #slide-counter { color:#58a6ff; font-size:0.85rem; margin-bottom:4px; }
  #slide-title { font-size:1.1rem; font-weight:600; }
  #slide-jump { display:flex; align-items:center; gap:8px; margin-top:8px; }
  #slide-jump input { width:60px; background:#0d1117; border:1px solid #30363d; color:#e6edf3;
                      border-radius:6px; padding:4px 8px; font-size:0.85rem; text-align:center; }
  #slide-jump button { background:#30363d; color:#e6edf3; border:none; border-radius:6px;
                       padding:4px 12px; font-size:0.85rem; cursor:pointer; }
  #content-preview { padding:12px 16px; flex:1; overflow-y:auto; }
  #content-preview ul { list-style:none; padding:0; margin:0; }
  #content-preview li { color:#8b949e; font-size:0.85rem; padding:2px 0; }
  #content-preview li::before { content:"\2022 "; color:#58a6ff; }
  #notes-section { border-top:1px solid #30363d; max-height:30vh; overflow-y:auto; }
  #notes-toggle { display:block; width:100%; background:#161b22; border:none; color:#8b949e;
                  padding:8px 16px; text-align:left; cursor:pointer; font-size:0.8rem;
                  font-family:inherit; border-top:1px solid #30363d; }
  #notes-toggle:hover { color:#e6edf3; }
  #notes-body { padding:8px 16px; color:#8b949e; font-size:0.85rem; white-space:pre-wrap;
                display:none; }
  #notes-body.open { display:block; }
  nav { background:#161b22; border-top:1px solid #30363d; padding:12px 16px;
        display:flex; gap:12px; }
  nav button { flex:1; font-size:1.1rem; padding:14px 0; border:none; border-radius:8px;
               cursor:pointer; font-family:inherit; font-weight:600; }
  .prev { background:#30363d; color:#e6edf3; }
  .prev:active { background:#484f58; }
  .next { background:#238636; color:#fff; }
  .next:active { background:#2ea043; }
  #status { text-align:center; padding:4px; font-size:0.7rem; color:#484f58; }
</style>
</head>
<body>
<header>
  <h1>Ostendo</h1>
  <span id="timer"></span>
</header>
<div id="slide-info">
  <div id="slide-counter">-- / --</div>
  <div id="slide-title">Connecting...</div>
  <div id="slide-jump">
    <input type="number" id="jump-input" min="1" placeholder="#">
    <button onclick="jumpToSlide()">Go</button>
  </div>
</div>
<div id="content-preview"></div>
<div id="notes-section">
  <button id="notes-toggle" onclick="toggleNotes()">Notes &#9660;</button>
  <div id="notes-body"></div>
</div>
<nav>
  <button class="prev" onclick="send('prev')">&#9664; Prev</button>
  <button class="next" onclick="send('next')">Next &#9654;</button>
</nav>
<div id="status"></div>
<script>
  let ws, touchStartX = 0;
  function connect() {
    ws = new WebSocket("ws://" + location.host);
    ws.onopen = () => { document.getElementById("status").textContent = "connected"; };
    ws.onmessage = e => {
      const d = JSON.parse(e.data);
      if (d.type === "state") {
        document.getElementById("slide-counter").textContent = "Slide " + d.slide + " / " + d.total;
        document.getElementById("slide-title").textContent = d.slide_title || "Untitled";
        document.getElementById("timer").textContent = d.timer || "";
        document.getElementById("notes-body").textContent = d.notes || "(no notes)";
        const ji = document.getElementById("jump-input");
        ji.max = d.total;
        ji.placeholder = d.slide;
        // Content preview
        const cp = document.getElementById("content-preview");
        if (d.slide_content && d.slide_content.length > 0) {
          cp.innerHTML = "<ul>" + d.slide_content.map(c => "<li>" + escHtml(c) + "</li>").join("") + "</ul>";
        } else { cp.innerHTML = ""; }
      }
    };
    ws.onclose = () => {
      document.getElementById("status").textContent = "reconnecting...";
      setTimeout(connect, 2000);
    };
    ws.onerror = () => { ws.close(); };
  }
  function send(action, slide) {
    if (ws && ws.readyState === 1) {
      const msg = {type:"command", action:action};
      if (slide !== undefined) msg.slide = slide;
      ws.send(JSON.stringify(msg));
    }
  }
  function jumpToSlide() {
    const v = parseInt(document.getElementById("jump-input").value);
    if (v > 0) send("goto", v);
    document.getElementById("jump-input").value = "";
  }
  function toggleNotes() {
    document.getElementById("notes-body").classList.toggle("open");
    const btn = document.getElementById("notes-toggle");
    btn.innerHTML = document.getElementById("notes-body").classList.contains("open")
      ? "Notes &#9650;" : "Notes &#9660;";
  }
  function escHtml(s) {
    return s.replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;");
  }
  document.addEventListener("keydown", e => {
    if (e.key === "ArrowLeft" || e.key === "h") send("prev");
    if (e.key === "ArrowRight" || e.key === "l" || e.key === " ") send("next");
    if (e.key === "Enter" && document.activeElement.id === "jump-input") jumpToSlide();
  });
  // Swipe support
  document.addEventListener("touchstart", e => { touchStartX = e.changedTouches[0].screenX; });
  document.addEventListener("touchend", e => {
    const dx = e.changedTouches[0].screenX - touchStartX;
    if (Math.abs(dx) > 50) { dx > 0 ? send("prev") : send("next"); }
  });
  connect();
</script>
</body>
</html>"##;
