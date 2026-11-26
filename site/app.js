"use strict";

const D = window.DATA;
const sizes = D.scale?.sizes || [];
const node = (id) => document.getElementById(id);
const label = (id) => D.labels[id] || id;
const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;");
const n = (x) => x.toLocaleString("en-US", {maximumFractionDigits: 0});
const sig = (x) => x === null || x === undefined ? "not measured" : x === 0 ? "0" : x.toPrecision(3).replace(/\.0+(?=e|$)/, "");
const pct = (x) => x === null ? "not reached" : `${sig(x * 100)}%`;
const span = (xs, fmt = sig) => xs?.length ? `${fmt(Math.min(...xs))}–${fmt(Math.max(...xs))}` : "not measured";
const mean = (xs) => xs.reduce((a, b) => a + b, 0) / xs.length;
const text = (x, y, s, cls = "", more = "") => `<text x="${x}" y="${y}" class="${cls}" ${more}>${esc(s)}</text>`;
const line = (x1, y1, x2, y2, cls = "grid") => `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" class="${cls}"/>`;
const dot = (x, y, tip, r = 4) => `<circle cx="${x}" cy="${y}" r="${r}" class="dot"><title>${esc(tip)}</title></circle>`;
const svg = (id, w, h, body, title) => { node(id).innerHTML = `<svg viewBox="0 0 ${w} ${h}" role="img" aria-label="${esc(title)}"><title>${esc(title)}</title>${body}</svg>`; };
const pending = (id, message) => { node(id).innerHTML = `<p class="pending">${esc(message)}</p>`; };
function table(caption, heads, rows, cls = "") {
  return `<table class="${cls}"><caption>${esc(caption)}</caption><thead><tr>${heads.map((h) => `<th scope="col">${esc(h)}</th>`).join("")}</tr></thead><tbody>${rows.map((r) => `<tr>${r.map((v) => `<td>${v}</td>`).join("")}</tr>`).join("")}</tbody></table>`;
}

function costChart(id, size, key) {
  const rows = Object.entries(size.engines).sort((a, b) => a[1][key] - b[1][key]);
  const w = Math.max(340, Math.min(560, node(id).clientWidth));
  const positive = rows.flatMap(([, v]) => [v[key], ...(v[`${key}_pass`] || [])]).filter((x) => x > 0);
  const lo = Math.floor(Math.log10(Math.min(...positive))), hi = Math.max(lo + 1, Math.ceil(Math.log10(Math.max(...positive))));
  const X = (x) => 16 + (Math.log10(x) - lo) / (hi - lo) * (w - 90);
  let body = "";
  const bottom = rows.length * 57 + 14;
  for (let t = lo; t <= hi; t++) body += line(X(10 ** t), 14, X(10 ** t), bottom) + text(X(10 ** t), bottom + 20, sig(10 ** t), "small", 'text-anchor="middle"');
  rows.forEach(([e, v], i) => {
    const y = i * 57 + 20, values = (v[`${key}_pass`] || []).filter((x) => x > 0);
    body += text(16, y, label(e));
    if (v[key] > 0) {
      if (values.length) body += line(X(Math.min(...values)), y + 17, X(Math.max(...values)), y + 17, "range");
      body += dot(X(v[key]), y + 17, `${label(e)}: ${sig(v[key])}; pass range ${span(values)}`);
    }
    body += text(w - 4, y + 21, sig(v[key]), "small", 'text-anchor="end"');
  });
  svg(id, w, bottom + 36, body, `${size.label}: ${key === "q" ? "request" : "update"} costs, logarithmic axis`);
}

function partial(size) {
  const rows = size.updates, groups = [...new Set(rows.filter((u) => u.target_f !== null).map((u) => u.target_f))].sort((a, b) => a - b);
  const pts = groups.map((f) => { const g = rows.filter((u) => u.target_f === f); return {f: mean(g.map((u) => u.f)), ratio: mean(g.map((u) => u.ratio))}; });
  const actual = rows.filter((u) => u.target_f === null && u.f > 0 && u.ratio > 0);
  const all = [...pts, ...actual], w = Math.max(340, Math.min(900, node("partial-chart").clientWidth)), h = 280;
  const ymin = Math.min(-1, Math.floor(Math.log10(Math.min(...all.map((p) => p.ratio)))));
  const ymax = Math.max(1, Math.ceil(Math.log10(Math.max(...all.map((p) => p.ratio)))));
  const X = (x) => 55 + (Math.log10(x) + 3) / 3 * (w - 90);
  const Y = (y) => h - 44 - (Math.log10(y) - ymin) / (ymax - ymin) * (h - 75);
  let body = "";
  for (let t = -3; t <= 0; t++) body += line(X(10 ** t), 20, X(10 ** t), h - 44) + text(X(10 ** t), h - 20, pct(10 ** t), "small", 'text-anchor="middle"');
  for (let t = ymin; t <= ymax; t++) body += line(55, Y(10 ** t), w - 35, Y(10 ** t), t === 0 ? "ref" : "grid") + text(47, Y(10 ** t) + 4, sig(10 ** t), "small", 'text-anchor="end"');
  body += `<polyline points="${pts.map((p) => `${X(p.f)},${Y(p.ratio)}`).join(" ")}" class="line"/>`;
  for (const p of pts) body += dot(X(p.f), Y(p.ratio), `Synthetic: ${pct(p.f)} changed, partial/full ${sig(p.ratio)}`);
  for (const p of actual) body += `<circle cx="${X(p.f)}" cy="${Y(p.ratio)}" r="3" fill="none" stroke="var(--warn)"><title>${esc(`${p.sequence}: ${p.from} → ${p.to}; ${pct(p.f)} changed, ratio ${sig(p.ratio)}`)}</title></circle>`;
  svg("partial-chart", w, h, body, `${size.label}: partial/full update ratio; filled synthetic, open dataset transitions`);
  node("partial-title").textContent = size.f_star === null ? `${size.label}: partial/full crossing was not reached` : size.f_star === 0 ? `${size.label}: partial updates already cost more at the smallest tested fraction` : `${size.label}: partial customization crosses full cost at ${pct(size.f_star)} changed arcs`;
  node("partial-table").innerHTML = table(`f* ${pct(size.f_star)}; pass range ${span(size.f_star_pass.filter((f) => f !== null), pct)}. Filled marks: synthetic; open marks: dataset.`, ["Sequence", "From → to", "Changed arcs", "Partial, s", "Full, s", "Ratio", "Pass ranges"], rows.map((u) => [esc(u.sequence), esc(`${u.from} → ${u.to}`), pct(u.f), sig(u.partial_s), sig(u.full_s), sig(u.ratio), u.distinguishable ? "separate" : "not distinguishable"]));
}

function boundaries() {
  const nation = sizes.find((s) => s.id === "korea");
  if (!nation) { pending("nation-chart", "Nationwide ranking not measured: the complete timing and memory population is unavailable."); return; }
  const w = Math.max(340, Math.min(900, node("nation-chart").clientWidth)), h = 350;
  const X = (r) => 60 + (Math.log10(r) - 3) / 6 * (w - 85);
  const Y = (k) => h - 45 - (Math.log10(k) + 2) / 8 * (h - 70);
  let body = "";
  for (let t = 3; t <= 9; t++) body += line(X(10 ** t), 20, X(10 ** t), h - 45) + text(X(10 ** t), h - 24, `10^${t}`, "small", 'text-anchor="middle"');
  for (let t = -2; t <= 6; t += 2) body += line(60, Y(10 ** t), w - 25, Y(10 ** t)) + text(52, Y(10 ** t) + 4, `10^${t}`, "small", 'text-anchor="end"');
  nation.switches.forEach((s) => {
    const points = Array.from({length:61}, (_, i) => 10 ** (3 + i / 10)).map((r) => ({r, k:s.k * r / 1e6})).filter((p) => p.k >= 0.01 && p.k <= 1e6);
    body += `<polyline points="${points.map((p) => `${X(p.r)},${Y(p.k)}`).join(" ")}" class="line"><title>${esc(`Switch to ${label(s.to)} at K/R = ${sig(s.k / 1e6)}`)}</title></polyline>`;
  });
  body += text(w / 2, h - 3, "R · requests per day", "small", 'text-anchor="middle"') + text(60, 13, "K · updates per day", "small");
  svg("nation-chart", w, h, body, "Nationwide all-engine cost boundaries, logarithmic axes; regions listed below");
  const order = [{k:0, to:nation.static}, ...nation.switches];
  node("nation-boundaries").innerHTML = table("Cheapest engine at one million daily requests; multiply K by R / one million for other volumes.", ["Updates per day", "Engine"], order.map((s, i) => [`${sig(s.k)} to ${i + 1 < order.length ? sig(order[i + 1].k) : "∞"}`, esc(label(s.to))]));
}

function growth() {
  const pts = sizes.filter((s) => s.k !== null && s.k > 0);
  if (!pts.length) { pending("k-chart", "K* awaits the complete comparison measurements."); return; }
  const w = Math.max(340, Math.min(900, node("k-chart").clientWidth)), h = 280;
  const lo = Math.floor(Math.log10(Math.min(...pts.map((s) => s.nodes)))), hi = Math.max(lo + 1, Math.ceil(Math.log10(Math.max(...pts.map((s) => s.nodes)))));
  const kl = Math.floor(Math.log10(Math.min(...pts.flatMap((s) => [s.k, ...s.k_pass]).filter((k) => k > 0))));
  const kh = Math.max(kl + 1, Math.ceil(Math.log10(Math.max(...pts.flatMap((s) => [s.k, ...s.k_pass]).filter((k) => k > 0)))));
  const X = (x) => 55 + (Math.log10(x) - lo) / (hi - lo) * (w - 85), Y = (k) => h - 45 - (Math.log10(k) - kl) / (kh - kl) * (h - 70);
  let body = "";
  for (let t = lo; t <= hi; t++) body += line(X(10 ** t), 20, X(10 ** t), h - 45) + text(X(10 ** t), h - 25, n(10 ** t), "small", 'text-anchor="middle"');
  for (let t = kl; t <= kh; t++) body += line(55, Y(10 ** t), w - 30, Y(10 ** t)) + text(47, Y(10 ** t) + 4, sig(10 ** t), "small", 'text-anchor="end"');
  pts.forEach((s) => { const ks = s.k_pass.filter((k) => k !== null && k > 0); if (ks.length) body += line(X(s.nodes), Y(Math.min(...ks)), X(s.nodes), Y(Math.max(...ks)), "range"); body += dot(X(s.nodes), Y(s.k), `${s.label}: ${n(s.nodes)} nodes, K* ${sig(s.k)}`); });
  body += text(w / 2, h - 3, "Network junctions", "small", 'text-anchor="middle"');
  svg("k-chart", w, h, body, "K* against network size; whiskers show pass ranges");
  node("k-table").innerHTML = table("Base-engine K* and all-engine static winner", ["Network", "Junctions", "K*", "Pass range", "Cheapest at K = 0"], sizes.map((s) => [esc(s.label), n(s.nodes), s.k === null ? "never" : sig(s.k), span(s.k_pass.filter((k) => k !== null)), esc(label(s.static))]));
}

function engineTables() {
  node("engine-tables").innerHTML = sizes.map((s) => `<h3>${esc(s.label)} · ${n(s.nodes)} junctions</h3><p class="note">${n(s.queries)} requests; reachable counts by timing slot/shard: ${[...new Set(s.reachable)].map(n).join(", ")}; ${n(s.identity)} identical endpoints. ${s.static_overlaps.length ? `Static query costs not distinguishable from ${s.static_overlaps.map((e) => esc(label(e))).join(", ")}.` : ""}</p><div class="scroll">${table("Mean costs, with pass ranges in parentheses. All answers checked; no re-priced industry rows accepted.", ["Engine", "q · core-h / billion", "u · core-s / update", "Update wall, s", "Preparation, s", "Memory, MB", "Checked answers"], Object.entries(s.engines).map(([e, v]) => [esc(label(e)), `${sig(v.q)} (${span(v.q_pass)})`, `${sig(v.u)} (${span(v.u_pass)})`, sig(v.u_wall) + (v.four_threads_wall_s === undefined ? "" : `<br>${sig(v.four_threads_wall_s)} (four threads)`), sig(v.prepare_s), v.memory_bytes !== null ? `${sig(v.memory_bytes / 1e6)} incremental` : v.process_peak_rss_bytes === undefined ? "not measured" : `${sig(v.process_peak_rss_bytes / 1e6)} process peak`, v.checked === null ? "not measured" : n(v.checked)]), "engine-table")}</div>`).join("");
}

function exclusions() {
  const evidence = D.industry;
  if (!evidence) { pending("exclusions", "Conversion-attempt evidence is not yet retained."); return; }
  node("exclusions").innerHTML = `<p>${esc(evidence.policy)}</p>` + evidence.engines.map((e) => `<h3>${esc(e.name)} ${esc(e.release)} · ${esc(e.status)}</h3><p><a href="${esc(e.licence_url)}">${esc(e.licence)}</a>; native build, commit <code>${esc(e.commit)}</code>.</p><ul>${e.attempts.map((a) => `<li><a href="${esc(a.url)}">Run ${esc(a.run)}</a>: ${esc(a.failure)}</li>`).join("")}</ul>`).join("") + `<p class="note">${esc(evidence.limit)}</p>`;
}

function renderSize() {
  const size = sizes.find((s) => s.id === node("size").value);
  if (!size) return;
  costChart("q-chart", size, "q"); costChart("u-chart", size, "u"); partial(size);
}

function render() {
  if (sizes.length) {
    const groups = [];
    sizes.forEach((s) => { const last = groups.at(-1); if (last?.engine === s.static) last.sizes.push(s); else groups.push({engine:s.static, sizes:[s]}); });
    const endpoint = (s) => `${s.label} (${n(s.nodes)} junctions)`;
    node("answer").textContent = "With fixed traffic, " + groups.map((g) => `${label(g.engine)} is cheapest ${g.sizes.length === 1 ? `at ${endpoint(g.sizes[0])}` : `from ${endpoint(g.sizes[0])} through ${endpoint(g.sizes.at(-1))}`}`).join("; ") + ".";
    node("coverage").textContent = D.scale.missing.length ? `Complete comparisons retained for ${n(sizes.length)} networks. Still missing: ${D.scale.missing.join(", ")}.` : `Complete comparisons retained for ${n(sizes.length)} networks, from city to nation.`;
    node("size").disabled = false;
    node("size").innerHTML = sizes.map((s) => `<option value="${esc(s.id)}">${esc(s.label)} · ${n(s.nodes)} junctions</option>`).join("");
    renderSize(); engineTables();
  } else {
    node("coverage").textContent = "Request, update, preparation and memory comparisons are pending a complete run with shared path-returning workloads. Earlier distance-only timings are excluded from this ranking.";
    for (const id of ["q-chart", "u-chart", "partial-chart", "engine-tables"]) pending(id, "Comparison measurements pending.");
  }
  boundaries(); growth(); exclusions();
  if (D.traffic) node("regret-table").innerHTML = table("Mean excess travel time across informative transitions; nationwide is a one-pass pilot, separate from the engine ranking", ["Network", "Requests", "Timing passes", "Reachable distinct trips", "Road-class free flow", "Previous consecutive slot", "Previous observed slot"], D.traffic.regions.map((r) => [esc(r.label), n(r.queries), n(r.timing_passes), n(r.series.free.pairs), pct(r.series.free.mean), pct(r.series.consecutive.mean), pct(r.series.observed.mean)]));
  else pending("regret-table", "Traffic regret not measured.");
}

render();
node("size").addEventListener("change", renderSize);
let frame;
window.addEventListener("resize", () => { cancelAnimationFrame(frame); frame = requestAnimationFrame(() => { renderSize(); boundaries(); growth(); }); });
