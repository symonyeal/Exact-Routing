"use strict";

(() => {
  const $ = (id) => document.getElementById(id);
  const data = window.ROUTING_INSIGHTS;
  if (!data) {
    $("headline-facts").textContent = "Processed evidence is unavailable. Run python scripts/insights.py to rebuild it from the frozen results.";
    return;
  }
  const state = { scale: 0, city: "sejong", bound: 256, stat: "p50", layer: "search", query: 2, version: 0, diagram: null };
  const labels = { walk8: "8-arc walk", walk32: "32-arc walk", uniform: "Uniform pairs" };
  const colors = { walk8: "#126f66", walk32: "#628ab2", uniform: "#bd7921" };
  const fmt = (v, digits = 0) => Number.isFinite(v) ? v.toLocaleString("en", { maximumFractionDigits: digits }) : "—";
  const pct = (v) => `${fmt(v * 100, 1)}%`;
  const ratio = (v) => `${v.toFixed(2)}×`;
  const city = () => data.cities.find((c) => c.id === state.city);
  const metric = () => city().metrics[String(state.scale)];
  const comparison = () => metric().comparisons.find((c) => c.bound === state.bound);
  const name = (c) => c.label.replace(/, large snapshot$/, "");
  const maps = window.PartitionMaps ? new window.PartitionMaps() : null;
  let controller;

  function svg(tag, attributes = {}, text) {
    const element = document.createElementNS("http://www.w3.org/2000/svg", tag);
    for (const [key, value] of Object.entries(attributes)) element.setAttribute(key, String(value));
    if (text !== undefined) element.textContent = String(text);
    return element;
  }
  function line(root, x1, y1, x2, y2, attributes = {}) {
    root.append(svg("line", { x1, y1, x2, y2, ...attributes }));
  }

  function renderEffect() {
    const summary = data.summary[String(state.scale)];
    $("headline-facts").innerHTML = [
      [pct(summary.median_boundary_reduction), "median boundary-node reduction across all 33 matched pairs"],
      [ratio(summary.median_mean_speedup), "median ratio of graph / hypergraph mean query time"],
      [ratio(summary.median_p50_speedup), "median ratio of graph / hypergraph p50 query time"],
    ].map(([value, label]) => `<div class="fact"><strong>${value}</strong><span>${label}</span></div>`).join("");
    const root = $("effect-chart"); root.replaceChildren();
    const width = Math.max(430, root.clientWidth || 900);
    root.setAttribute("viewBox", `0 0 ${width} 340`);
    const rows = data.cities.flatMap((c) => c.metrics[String(state.scale)].comparisons.map((r) => ({ ...r, city: c })));
    const key = `${state.stat}_speedup`, lo = Math.min(.8, ...rows.map((r) => r[key])) - .05;
    const hi = Math.max(1.75, ...rows.map((r) => r[key])) + .08;
    const y = (v) => 270 - (v - lo) / (hi - lo) * 235;
    const xs = [95, width / 2, width - 65];
    for (const value of [.8, 1, 1.2, 1.4, 1.6, 1.8, 2]) if (value >= lo && value <= hi) {
      line(root, 75, y(value), width - 30, y(value), { stroke: value === 1 ? "#9baea5" : "#dce3db", "stroke-dasharray": value === 1 ? "4 4" : "none" });
      root.append(svg("text", { x: 62, y: y(value) + 4, "text-anchor": "end", class: "svg-small" }, `${value.toFixed(1)}×`));
    }
    root.append(svg("text", { x: 75, y: 16, class: "svg-title" }, `Graph / hypergraph ${state.stat === "p50" ? "median" : "mean"} latency`));
    for (const c of data.cities) {
      const selected = c.id === state.city;
      const rr = c.metrics[String(state.scale)].comparisons;
      const points = rr.map((r, i) => `${xs[i]},${y(r[key])}`).join(" ");
      root.append(svg("polyline", { points, fill: "none", stroke: selected ? "#174c49" : "#b5c4bd", "stroke-width": selected ? 2.6 : 1, opacity: selected ? 1 : .65 }));
      rr.forEach((r, i) => {
        const item = svg("g", { class: "point", tabindex: 0, role: "button", "aria-label": `${name(c)}, U ${r.bound}, ${ratio(r[key])} graph divided by hypergraph time` });
        item.append(svg("circle", { cx: xs[i], cy: y(r[key]), r: selected && r.bound === state.bound ? 8 : selected ? 5 : 3.4, fill: selected ? "#174c49" : "#a4b9af", stroke: selected && r.bound === state.bound ? "#fffdf5" : "none", "stroke-width": 2 }));
        item.append(svg("title", {}, `${name(c)} · U=${r.bound}\nBoundary reduction ${pct(r.boundary_reduction)}\nGraph / H time ${ratio(r[key])}`));
        const choose = () => select(c.id, r.bound);
        item.addEventListener("click", choose);
        item.addEventListener("keydown", (e) => { if (["Enter", " "].includes(e.key)) { e.preventDefault(); choose(); } });
        root.append(item);
      });
    }
    const aggregate = summary.by_bound;
    root.append(svg("polyline", { points: aggregate.map((r, i) => `${xs[i]},${y(r[`median_${state.stat}_speedup`])}`).join(" "), fill: "none", stroke: "#bc7621", "stroke-width": 3.5 }));
    aggregate.forEach((r, i) => {
      root.append(svg("circle", { cx: xs[i], cy: y(r[`median_${state.stat}_speedup`]), r: 5, fill: "#bc7621" }));
      root.append(svg("text", { x: xs[i], y: 305, "text-anchor": "middle", class: "svg-label" }, `U = ${fmt(r.bound)}`));
    });
    root.append(svg("text", { x: width / 2, y: 334, "text-anchor": "middle", class: "svg-small" }, "Cell bound · selected city in teal · median of ratios in amber"));
    $("bound-table").replaceChildren();
    aggregate.forEach((r) => {
      const tr = document.createElement("tr"); if (r.bound === state.bound) tr.className = "active";
      tr.innerHTML = `<td><button>${fmt(r.bound)}</button></td><td>${pct(r.median_boundary_reduction)}</td><td>${r[`${state.stat}_wins`]} / ${r.count}</td><td>${ratio(r[`median_${state.stat}_speedup`])}</td>`;
      tr.querySelector("button").addEventListener("click", () => select(state.city, r.bound));
      $("bound-table").append(tr);
    });
    const small = aggregate[0], large = aggregate[2];
    $("crossover-note").textContent = `${small[`${state.stat}_wins`]} of 11 cities favor hypergraph at U=256; ${large[`${state.stat}_wins`]} of 11 at U=4,096. Boundary savings stay near ${pct(large.median_boundary_reduction)} at the larger bound.`;
  }

  function renderSelection() {
    const c = city(), r = comparison();
    $("city-select").value = c.id; $("bound-select").value = String(state.bound);
    $("source-link").href = c.source_url;
    $("selected-finding").textContent = `${name(c)} · U=${fmt(state.bound)}: boundaries ${fmt(r.graph.boundary)} → ${fmt(r.hypergraph.boundary)} (${pct(r.boundary_reduction)} fewer); overlay arcs ${fmt(r.graph.overlay_arcs)} → ${fmt(r.hypergraph.overlay_arcs)}. Graph / H ${state.stat === "p50" ? "median" : "mean"} time: ${ratio(r[`${state.stat}_speedup`])}.`;
    for (const [key, short] of [["graph", "graph"], ["hypergraph", "hypergraph"]]) {
      $(`${short}-boundary`).innerHTML = `${fmt(r[key].boundary)}<small>boundary nodes · ${fmt(r[key].cells)} cells</small>`;
    }
    const presets = metric().workload.presets;
    if (!presets.some((p) => p.index === state.query)) state.query = presets[0].index;
    $("query-select").replaceChildren();
    for (const p of presets) {
      const option = document.createElement("option"); option.value = String(p.index);
      option.textContent = `#${p.index + 1} · ${labels[p.category]} · ${p.distance_seconds === null ? "unreachable" : `${fmt(p.distance_seconds, 2)} s`}${p.source === p.target ? " · same endpoint" : ""}`;
      $("query-select").append(option);
    }
    $("query-select").value = String(state.query);
    renderWorkload(); renderBaselines(); renderEffect();
  }

  function renderWorkload() {
    const groups = metric().workload.groups;
    $("workload-table").innerHTML = groups.map((g) => `<tr><td>${labels[g.id]}</td><td>${fmt(g.count)}</td><td>${fmt(g.identical)}</td><td>${fmt(g.median_seconds, 2)}</td></tr>`).join("");
    const root = $("cost-chart"); root.replaceChildren();
    const width = Math.max(430, root.clientWidth || 800);
    root.setAttribute("viewBox", `0 0 ${width} 290`);
    const max = Math.max(1, ...groups.flatMap((g) => g.distances_seconds.filter((v) => v !== null)));
    const top = Math.log1p(max), x = (v) => 60 + Math.log1p(v) / top * (width - 100), y = (v) => 235 - v * 200;
    for (const q of [0, .25, .5, .75, 1]) {
      line(root, 60, y(q), width - 40, y(q), { stroke: "#dce3db" });
      root.append(svg("text", { x: 48, y: y(q) + 4, "text-anchor": "end", class: "svg-small" }, `${q * 100}%`));
    }
    for (const v of [0, 1, 10, 100, 1000, 10000]) if (v <= max) {
      root.append(svg("text", { x: x(v), y: 258, "text-anchor": "middle", class: "svg-small" }, fmt(v)));
    }
    groups.forEach((g, i) => {
      const values = g.distances_seconds.filter((v) => v !== null).sort((a, b) => a - b);
      const points = values.map((v, j) => `${x(v)},${y((j + 1) / values.length)}`).join(" ");
      root.append(svg("polyline", { points, fill: "none", stroke: colors[g.id], "stroke-width": 2.4 }));
      root.append(svg("text", { x: 65 + i * (width - 90) / 3, y: 18, fill: colors[g.id], "font-size": 12 }, labels[g.id]));
    });
    root.append(svg("text", { x: width / 2, y: 286, "text-anchor": "middle", class: "svg-small" }, "Travel seconds · each curve uses its reachable cases"));
  }

  function renderBaselines() {
    const r = comparison(), stat = state.stat === "p50" ? "p50_us" : "mean_us";
    const rows = [{ label: `Graph overlay · U=${fmt(state.bound)}`, value: r.graph[stat], type: "" }, { label: `Hypergraph overlay · U=${fmt(state.bound)}`, value: r.hypergraph[stat], type: "hyper" }, ...metric().baselines.map((b) => ({ label: b.method === "dijkstra" ? "Dijkstra" : b.method === "bidirectional" ? "Bidirectional Dijkstra" : "Contraction hierarchy", value: b[stat], type: b.method.includes("CH") ? "ch" : "" }))].sort((a, b) => a.value - b.value);
    const max = Math.max(...rows.map((r) => r.value));
    $("baseline-bars").innerHTML = rows.map((r) => `<div class="baseline-row ${r.type}"><span>${r.label}</span><span class="baseline-track"><i style="width:${100 * r.value / max}%"></i></span><strong>${fmt(r.value, 2)} µs</strong></div>`).join("");
  }

  function parseNetwork(buffer) {
    const view = new DataView(buffer), u64 = (at) => view.getUint32(at, true) + view.getUint32(at + 4, true) * 4294967296;
    const n = u64(0), m = u64(8);
    if (!n || !Number.isSafeInteger(n) || !Number.isSafeInteger(m) || buffer.byteLength !== 16 + 16 * (n + m)) throw new Error("Road dimensions do not match the binary.");
    const lat = new Float64Array(n), lon = new Float64Array(n); let cy = 0, cx = 0;
    for (let i = 0; i < n; i++) { lat[i] = view.getFloat64(16 + i * 8, true); lon[i] = view.getFloat64(16 + n * 8 + i * 8, true); cy += lat[i] / n; cx += lon[i] / n; }
    const cos = Math.cos(cy * Math.PI / 180), points = new Float32Array(n * 2);
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (let i = 0; i < n; i++) {
      const x = (lon[i] - cx) * cos * 111.32, y = (lat[i] - cy) * 111.32;
      if (!Number.isFinite(x) || !Number.isFinite(y)) throw new Error("Road coordinates are invalid.");
      points[2 * i] = x; points[2 * i + 1] = y;
      minX = Math.min(minX, x); maxX = Math.max(maxX, x); minY = Math.min(minY, y); maxY = Math.max(maxY, y);
    }
    return { n, points, bounds: { minX, maxX, minY, maxY } };
  }

  async function get(url, signal) {
    const response = await fetch(url, { signal });
    if (!response.ok) { let message = response.statusText; try { message = (await response.json()).error || message; } catch { /* plain response */ } throw new Error(message); }
    return response;
  }
  function status(message, error = false, retry = false) {
    const box = $("diagram-status"); box.className = `diagram-status${error ? " error" : ""}`; box.replaceChildren(document.createTextNode(message));
    if (retry) { const button = document.createElement("button"); button.textContent = "Retry diagnostic"; button.addEventListener("click", loadDiagram); box.append(button); }
  }
  async function loadDiagram() {
    const version = ++state.version;
    if (controller) controller.abort(); controller = new AbortController();
    const signal = controller.signal;
    state.diagram = null; maps?.clear(); $("work-bars").replaceChildren(); $("graph-work").textContent = ""; $("hypergraph-work").textContent = ""; $("query-evidence").textContent = "";
    if (location.protocol === "file:") { status("The evidence charts work offline. For the Rust spatial proof, run python scripts/serve_maps.py and open http://127.0.0.1:8765/."); return; }
    status(`Building or loading the actual ${name(city())} partitions at U=${fmt(state.bound)}. Counts are checked before drawing…`);
    try {
      const [road, exportResponse] = await Promise.all([get(`/api/network?city=${encodeURIComponent(state.city)}`, signal), get(`/api/partition?city=${encodeURIComponent(state.city)}&bound=${state.bound}&scale=${state.scale}`, signal)]);
      const [network, diagnostic] = await Promise.all([road.arrayBuffer().then(parseNetwork), exportResponse.json()]);
      if (version !== state.version) return;
      if (diagnostic.city !== state.city || diagnostic.bound !== state.bound || diagnostic.scale !== state.scale || diagnostic.nodes !== network.n) throw new Error("Diagnostic belongs to a different graph or configuration.");
      if (!maps) throw new Error("Partition renderer is unavailable.");
      state.diagram = diagnostic; maps.setData(network, diagnostic); maps.setLayer(state.layer); renderQuery();
      status(`Verified Rust export · ${fmt(diagnostic.models.Graph.cells)} graph cells / ${fmt(diagnostic.models.Hypergraph.cells)} hypergraph cells · ${diagnostic.queries.length} certified query traces.`);
    } catch (error) { if (error.name !== "AbortError" && version === state.version) status(`Spatial proof unavailable: ${error.message}`, true, true); }
  }
  function renderQuery() {
    const q = state.diagram?.queries.find((q) => q.index === state.query); if (!q) return;
    maps.setQuery(q);
    for (const [model, id] of [["Graph", "graph-work"], ["Hypergraph", "hypergraph-work"]]) {
      const r = q[model];
      $(id).innerHTML = `<span><b>${fmt(r.settled)}</b> settled</span><span>${fmt(r.boundary_settled)} boundary · ${fmt(r.interior_settled)} interior</span>`;
    }
    const seconds = q.distance === null ? null : q.distance / (state.scale || 1);
    $("query-evidence").textContent = `Query #${q.index + 1} · ${labels[q.category]} · boundary work ${fmt(q.Graph.boundary_settled)} → ${fmt(q.Hypergraph.boundary_settled)}; interior work ${fmt(q.Graph.interior_settled)} → ${fmt(q.Hypergraph.interior_settled)}. ${seconds === null ? "Certified unreachable." : `Both distances match the ${fmt(seconds, 3)} s certified reference.`}`;
    const maximum = Math.max(q.dijkstra.settled, q.Graph.settled, q.Hypergraph.settled, 1);
    $("work-bars").innerHTML = [["Dijkstra", q.dijkstra.settled, ""], ["Graph overlay", q.Graph.settled, ""], ["Hypergraph overlay", q.Hypergraph.settled, "hyper"]].map(([label, count, cls]) => `<div class="work-bar ${cls}">${label}<strong>${fmt(count)} settled nodes</strong><div class="work-track"><i style="width:${count / maximum * 100}%"></i></div></div>`).join("");
  }

  function renderOrder() {
    const order = data.order_experiment; if (!order?.experiments?.length) return;
    $("order-study").hidden = false;
    const select = $("order-city");
    if (!select.options.length) order.experiments.forEach((e) => { const option = document.createElement("option"); option.value = e.city; option.textContent = name(data.cities.find((c) => c.id === e.city)); select.append(option); });
    const e = order.experiments.find((e) => e.city === select.value) || order.experiments[0];
    const method = $("order-method").value, rows = e.results.filter((r) => r.method === method);
    const fixed = rows.find((r) => r.condition === "fixed"), reversed = rows.find((r) => r.condition === "reversed");
    const f8 = fixed.groups.find((g) => g.category === "walk8"), r8 = reversed.groups.find((g) => g.category === "walk8");
    $("order-finding").textContent = `The same 8-arc-walk queries have ${fmt(f8.median_query_us, 2)} µs median cost in fixed order and ${fmt(r8.median_query_us, 2)} µs in reversed order. Current-query settled and scanned counts are identical; the previous touched state changes from ${fmt(f8.median_previous_touched)} to ${fmt(r8.median_previous_touched)} entries.`;
    $("order-chain").innerHTML = [["8-arc walk", "clears previous uniform-query state"], ["32-arc walk", "clears previous 8-arc-walk state"], ["Uniform pair A", "clears previous 32-arc-walk state"], ["Uniform pair B", "clears previous uniform-query state"]].map(([a, b]) => `<div class="order-step">${a}<small>${b}</small></div>`).join("");
    $("order-table").innerHTML = `<div class="order-table-wrap"><table class="compact-table"><thead><tr><th>Condition / group</th><th>Query median, µs</th><th>Preparation median, µs</th><th>Total median, µs</th><th>Previous touched</th></tr></thead><tbody>${rows.flatMap((r) => r.groups.map((g) => `<tr><td>${r.condition} · ${labels[g.category]}</td><td>${fmt(g.median_query_us, 2)}</td><td>${fmt(g.median_preparation_us, 2)}</td><td>${fmt(g.median_total_us, 2)}</td><td>${fmt(g.median_previous_touched)}</td></tr>`)).join("")}</tbody></table></div>`;
    const p = order.provenance;
    $("order-provenance").textContent = `Local diagnostic · ${p.cpu} · ${e.queries} frozen pairs · ${e.repeats} passes · native seconds · condition order rotates between passes. Source and executable hashes, compiler and raw per-query means are retained in results/query-order.json. Medians in separate columns are computed independently.`;
  }

  function select(id, bound) { state.city = id; state.bound = Number(bound); renderSelection(); loadDiagram(); }
  for (const c of data.cities) { const option = document.createElement("option"); option.value = c.id; option.textContent = `${name(c)} · ${c.country}`; $("city-select").append(option); }
  $("city-select").addEventListener("change", () => select($("city-select").value, state.bound));
  $("bound-select").addEventListener("change", () => select(state.city, $("bound-select").value));
  $("stat").addEventListener("change", () => { state.stat = $("stat").value; renderSelection(); });
  $("query-select").addEventListener("change", () => { state.query = Number($("query-select").value); renderQuery(); });
  $("fit-map").addEventListener("click", () => maps?.fit());
  for (const button of document.querySelectorAll("[data-layer]")) button.addEventListener("click", () => { state.layer = button.dataset.layer; for (const other of document.querySelectorAll("[data-layer]")) other.setAttribute("aria-pressed", String(other === button)); maps?.setLayer(state.layer); });
  for (const button of document.querySelectorAll("[data-scale]")) button.addEventListener("click", () => {
    state.scale = Number(button.dataset.scale); for (const other of document.querySelectorAll("[data-scale]")) other.setAttribute("aria-pressed", String(other === button));
    $("metric-note").textContent = state.scale ? "Every arc becomes max(1, round(1000 × seconds)) milliseconds. This changes the objective." : "Source arc costs; floating-point answers checked within the declared tolerance.";
    renderSelection(); loadDiagram();
  });
  $("order-city").addEventListener("change", renderOrder); $("order-method").addEventListener("change", renderOrder);
  window.addEventListener("resize", () => { renderEffect(); renderWorkload(); });
  const example = data.order_experiment?.experiments?.find((e) => e.city === "sejong");
  if (example) {
    const group = (condition) => example.results.find((r) => r.condition === condition && r.method === "dijkstra").groups.find((g) => g.category === "walk8");
    $("cleanup-lede").textContent = `Sejong, native seconds: the same 8-arc-walk queries shift from ${fmt(group("fixed").median_query_us, 1)} µs to ${fmt(group("reversed").median_query_us, 1)} µs median when order is reversed. Current search work is identical. The cost moves between groups; preparation remains part of the total.`;
  }
  $("provenance").textContent = `${data.queries_per_graph.toLocaleString()} pairs per graph/metric; ${data.repeats} passes; seed ${data.seed}. Benchmark producer ${data.producer_sha.slice(0, 12)}. Input snapshot SHA-256 ${data.source_sha256}.`;
  renderSelection(); renderOrder(); loadDiagram();
})();
