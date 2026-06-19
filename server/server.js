// PatchPilot fleet dashboard + report API.
// Zero dependencies — runs on plain Node. Caddy terminates TLS and serves /updates.
//
//   PORT       listen port (default 8787)
//   DATA_DIR   where reports.json lives (default ./data)
//
// Routes:
//   POST /api/report     ingest a machine status report (from the app)
//   GET  /api/machines   JSON list of known machines
//   GET  /               HTML dashboard

import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const PORT = Number(process.env.PORT || 8787);
const DATA_DIR = process.env.DATA_DIR || join(process.cwd(), "data");
const DB = join(DATA_DIR, "reports.json");

mkdirSync(DATA_DIR, { recursive: true });
if (!existsSync(DB)) writeFileSync(DB, "{}");

const load = () => {
  try {
    return JSON.parse(readFileSync(DB, "utf8"));
  } catch {
    return {};
  }
};
const save = (obj) => writeFileSync(DB, JSON.stringify(obj, null, 2));

function readBody(req) {
  return new Promise((resolve, reject) => {
    let data = "";
    req.on("data", (c) => {
      data += c;
      if (data.length > 1_000_000) reject(new Error("too large"));
    });
    req.on("end", () => resolve(data));
    req.on("error", reject);
  });
}

const json = (res, code, obj) => {
  const body = JSON.stringify(obj);
  res.writeHead(code, {
    "content-type": "application/json",
    "access-control-allow-origin": "*",
  });
  res.end(body);
};

const server = createServer(async (req, res) => {
  const url = new URL(req.url, "http://x");

  if (req.method === "POST" && url.pathname === "/api/report") {
    try {
      const r = JSON.parse(await readBody(req));
      if (!r.hostname) return json(res, 400, { error: "hostname required" });
      const db = load();
      db[r.hostname] = { ...r, receivedAt: new Date().toISOString() };
      save(db);
      return json(res, 200, { ok: true });
    } catch (e) {
      return json(res, 400, { error: String(e) });
    }
  }

  if (req.method === "GET" && url.pathname === "/api/machines") {
    const db = load();
    const list = Object.values(db).sort((a, b) =>
      (a.hostname || "").localeCompare(b.hostname || "")
    );
    return json(res, 200, list);
  }

  if (req.method === "GET" && (url.pathname === "/" || url.pathname === "/index.html")) {
    res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    return res.end(DASHBOARD_HTML);
  }

  res.writeHead(404, { "content-type": "text/plain" });
  res.end("Not found");
});

server.listen(PORT, () => console.log(`PatchPilot dashboard on :${PORT}`));

const DASHBOARD_HTML = `<!doctype html>
<html lang="en"><head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>PatchPilot Fleet</title>
<style>
:root{font-family:"Segoe UI",system-ui,sans-serif;color:#e8edf6}
*{box-sizing:border-box}
body{margin:0;background:
 radial-gradient(1200px 700px at 80% -10%,rgba(99,102,241,.18),transparent 60%),
 radial-gradient(900px 600px at -10% 10%,rgba(59,130,246,.14),transparent 55%),
 linear-gradient(180deg,#0e1525,#0a0f1c);min-height:100vh;padding:28px}
h1{font-size:24px;margin:0 0 4px}
.sub{color:#97a3b8;margin-bottom:24px;font-size:13px}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(280px,1fr));gap:16px}
.card{background:rgba(255,255,255,.04);border:1px solid rgba(255,255,255,.09);
 border-radius:16px;padding:18px;position:relative;overflow:hidden}
.card::before{content:"";position:absolute;left:0;top:0;bottom:0;width:3px;background:var(--c)}
.name{font-size:16px;font-weight:700}
.meta{color:#97a3b8;font-size:12px;margin:2px 0 12px}
.pills{display:flex;gap:6px;flex-wrap:wrap}
.pill{font-size:12px;font-weight:650;padding:4px 10px;border-radius:999px;
 background:rgba(255,255,255,.06);border:1px solid rgba(255,255,255,.09)}
.foot{color:#5e6b82;font-size:11px;margin-top:12px}
.reboot{color:#fbbf24;font-weight:650}
.empty{color:#5e6b82;margin-top:40px}
</style></head><body>
<h1>PatchPilot Fleet</h1>
<div class="sub">Status reported by your machines · auto-refreshes every 30s</div>
<div id="grid" class="grid"></div>
<div id="empty" class="empty" style="display:none">No machines have reported yet.</div>
<script>
const ageColor=(iso)=>{const h=(Date.now()-new Date(iso))/3.6e6;
 return h<26?"#22c55e":h<24*8?"#f59e0b":"#ef4444"};
const ago=(iso)=>{const m=(Date.now()-new Date(iso))/6e4;
 if(m<60)return Math.round(m)+"m ago";if(m<1440)return Math.round(m/60)+"h ago";
 return Math.round(m/1440)+"d ago"};
async function load(){
 const r=await fetch("/api/machines");const list=await r.json();
 const grid=document.getElementById("grid");const empty=document.getElementById("empty");
 empty.style.display=list.length?"none":"block";
 grid.innerHTML=list.map(m=>{
  const c=m.fail>0?"#ef4444":m.warn>0?"#f59e0b":"#22c55e";
  const seen=m.receivedAt||m.timestamp;
  return \`<div class="card" style="--c:\${c}">
   <div class="name">\${m.hostname||"?"}</div>
   <div class="meta">\${[m.manufacturer,m.model].filter(Boolean).join(" ")} · \${m.os||""} · v\${m.version||"?"}</div>
   <div class="pills">
    <span class="pill" style="color:#22c55e">✓ \${m.ok??0}</span>
    <span class="pill" style="color:#f59e0b">⚠ \${m.warn??0}</span>
    <span class="pill" style="color:#ef4444">✗ \${m.fail??0}</span>
    <span class="pill" style="color:#94a3b8">○ \${m.skip??0}</span>
   </div>
   \${m.rebootRequired?'<div class="foot reboot">⚠ restart pending</div>':""}
   <div class="foot">last run \${m.mode||""} · \${seen?ago(seen):"?"}</div>
  </div>\`;
 }).join("");
}
load();setInterval(load,30000);
</script>
</body></html>`;
