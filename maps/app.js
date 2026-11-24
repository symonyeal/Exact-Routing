const $ = (id) => document.getElementById(id);

const el = {
  city: $("city-select"), title: $("city-title"), source: $("source-link"),
  origin: $("origin-input"), destination: $("destination-input"), pick: $("pick-target"),
  preset: $("preset-select"), routeForm: $("route-form"), routeButton: $("route-button"),
  routeResult: $("route-result"), clear: $("clear-route"), swap: $("swap-nodes"),
  metricNote: $("metric-note"), stat: $("stat-select"), benchContext: $("benchmark-context"),
  benchRows: $("benchmark-rows"), buildRows: $("build-rows"), machine: $("machine-note"),
  coverage: $("coverage-note"), stage: $("map-stage"), map: $("map"),
  canvas: $("network-canvas"), overlay: $("overlay-canvas"), mapState: $("map-state"),
  scaleBar: $("scale-bar"), timeLegend: $("time-legend"), timeRange: $("time-range"), cityNote: $("city-note"),
};

const state = {
  cities: [], city: null, network: null, benchmark: null, version: 0, benchVersion: 0,
  routeVersion: 0, route: null, view: { x: 0, y: 0, scale: 1 },
  mode: "topology", gl: null, glProgram: null, glBuffers: null, glNetwork: null,
  fallback: null, pointer: null, loading: false,
};

const glsl = {
  vertex: `attribute vec2 a_pos;
attribute vec3 a_color;
uniform vec2 u_center;
uniform vec2 u_resolution;
uniform float u_scale;
uniform float u_time;
uniform vec3 u_base;
varying vec3 v_color;
void main() {
  vec2 p = (a_pos - u_center) * u_scale;
  gl_Position = vec4(2.0 * p.x / u_resolution.x, 2.0 * p.y / u_resolution.y, 0.0, 1.0);
  v_color = mix(u_base, a_color, u_time);
}`,
  fragment: `precision mediump float;
varying vec3 v_color;
void main() { gl_FragColor = vec4(v_color, 0.86); }`,
};

function setMapState(message, kind = "loading", retry = false) {
  el.mapState.replaceChildren();
  if (!message) return;
  el.mapState.dataset.state = kind;
  const mark = document.createElement("span");
  mark.className = "state-mark";
  mark.setAttribute("aria-hidden", "true");
  const label = document.createElement("span");
  label.textContent = message;
  el.mapState.append(mark, label);
  if (retry) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = "Retry";
    button.addEventListener("click", () => state.cities.length ? loadCity() : loadCities());
    el.mapState.append(button);
  }
}

async function responseError(response) {
  let detail = "";
  try {
    const body = await response.text();
    try { detail = JSON.parse(body).error || ""; } catch { detail = body.trim().slice(0, 180); }
  } catch { /* no response body */ }
  return detail || `${response.status} ${response.statusText}`;
}

async function get(url, options = {}) {
  const response = await fetch(url, options);
  if (!response.ok) throw new Error(await responseError(response));
  return response;
}

function initRenderer() {
  const gl = el.canvas.getContext("webgl", { alpha: false, antialias: true, powerPreference: "high-performance" });
  if (!gl) {
    state.fallback = el.canvas.getContext("2d");
    if (state.fallback) return;
    setMapState("This browser cannot render the road graph.", "error");
    return;
  }
  try {
    const program = gl.createProgram();
    const compile = (type, source) => {
      const shader = gl.createShader(type);
      gl.shaderSource(shader, source);
      gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader));
      gl.attachShader(program, shader);
    };
    compile(gl.VERTEX_SHADER, glsl.vertex);
    compile(gl.FRAGMENT_SHADER, glsl.fragment);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
    state.gl = gl;
    state.glProgram = program;
    state.glBuffers = { pos: gl.createBuffer(), color: gl.createBuffer() };
    state.glInfo = {
      pos: gl.getAttribLocation(program, "a_pos"), color: gl.getAttribLocation(program, "a_color"),
      center: gl.getUniformLocation(program, "u_center"), resolution: gl.getUniformLocation(program, "u_resolution"),
      scale: gl.getUniformLocation(program, "u_scale"), time: gl.getUniformLocation(program, "u_time"),
      base: gl.getUniformLocation(program, "u_base"),
    };
    gl.useProgram(program);
    gl.clearColor(11 / 255, 25 / 255, 33 / 255, 1);
  } catch {
    setMapState("WebGL could not initialize. Reload in a browser with WebGL enabled.", "error");
  }
}

function parseNetwork(buffer, city) {
  if (buffer.byteLength < 16) throw new Error("Network binary is shorter than its header.");
  const view = new DataView(buffer);
  const u64 = (at) => view.getUint32(at, true) + view.getUint32(at + 4, true) * 4294967296;
  const n = u64(0), m = u64(8);
  if (!Number.isSafeInteger(n) || !Number.isSafeInteger(m) || n > 0xffffffff) throw new Error("Network size is outside the supported format.");
  const expected = 16 + 16 * n + 16 * m;
  if (!Number.isSafeInteger(expected) || buffer.byteLength !== expected) throw new Error("Network binary length does not match its header.");

  let centerLat = 0, centerLon = 0;
  const lat = new Float64Array(n), lon = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    lat[i] = view.getFloat64(16 + i * 8, true);
    lon[i] = view.getFloat64(16 + n * 8 + i * 8, true);
    if (!Number.isFinite(lat[i]) || !Number.isFinite(lon[i])) throw new Error(`Node ${i} has invalid coordinates.`);
    centerLat += lat[i] / Math.max(1, n);
    centerLon += lon[i] / Math.max(1, n);
  }
  const points = new Float32Array(n * 2);
  const cosLat = Math.cos(centerLat * Math.PI / 180);
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
  for (let i = 0; i < n; i++) {
    const x = (lon[i] - centerLon) * cosLat * 111.32;
    const y = (lat[i] - centerLat) * 111.32;
    points[2 * i] = x;
    points[2 * i + 1] = y;
    minX = Math.min(minX, x); maxX = Math.max(maxX, x);
    minY = Math.min(minY, y); maxY = Math.max(maxY, y);
  }
  const tailAt = 16 + 16 * n;
  const headAt = tailAt + 4 * m;
  const weightAt = headAt + 4 * m;
  let minWeight = Infinity, maxWeight = -Infinity;
  let minSeconds = Infinity, maxSeconds = -Infinity;
  for (let i = 0; i < m; i++) {
    const u = view.getUint32(tailAt + i * 4, true), v = view.getUint32(headAt + i * 4, true);
    const weight = view.getFloat64(weightAt + i * 8, true);
    if (u >= n || v >= n) throw new Error(`Arc ${i} refers to a missing node.`);
    if (!Number.isFinite(weight) || weight < 0) throw new Error(`Arc ${i} has an invalid weight.`);
    if (u !== v) {
      minWeight = Math.min(minWeight, Math.log1p(weight)); maxWeight = Math.max(maxWeight, Math.log1p(weight));
      minSeconds = Math.min(minSeconds, weight); maxSeconds = Math.max(maxSeconds, weight);
    }
  }
  const positions = new Float32Array(m * 4);
  const colors = new Float32Array(m * 6);
  const bins = new Uint8Array(m);
  let line = 0;
  const mix = (a, b, t) => a + (b - a) * t;
  for (let i = 0; i < m; i++) {
    const u = view.getUint32(tailAt + i * 4, true), v = view.getUint32(headAt + i * 4, true);
    if (u === v) continue;
    const ui = u * 2, vi = v * 2;
    positions.set([points[ui], points[ui + 1], points[vi], points[vi + 1]], line * 4);
    const w = Math.log1p(view.getFloat64(weightAt + i * 8, true));
    const q = maxWeight > minWeight ? (w - minWeight) / (maxWeight - minWeight) : 0.5;
    const color = [mix(0.19, 0.89, q), mix(0.55, 0.65, q), mix(0.53, 0.30, q)];
    bins[line] = Math.min(11, Math.floor(q * 12));
    colors.set(color, line * 6); colors.set(color, line * 6 + 3);
    line++;
  }
  return {
    city, n, m, points, positions: positions.subarray(0, line * 4), colors: colors.subarray(0, line * 6), bins: bins.subarray(0, line),
    lines: line, minSeconds, maxSeconds, bounds: { minX, maxX, minY, maxY },
  };
}

function niceStep(value) {
  if (!Number.isFinite(value) || value <= 0) return 1;
  const power = 10 ** Math.floor(Math.log10(value));
  const ratio = value / power;
  return (ratio < 2 ? 1 : ratio < 5 ? 2 : 5) * power;
}

function fitGraph() {
  const net = state.network;
  if (!net || !net.n) { draw(); return; }
  const b = net.bounds;
  state.view.x = (b.minX + b.maxX) / 2;
  state.view.y = (b.minY + b.maxY) / 2;
  const width = Math.max(1, el.stage.clientWidth - 80);
  const height = Math.max(1, el.stage.clientHeight - 100);
  const spanX = Math.max(0.2, b.maxX - b.minX);
  const spanY = Math.max(0.2, b.maxY - b.minY);
  state.view.scale = Math.min(width / spanX, height / spanY);
  draw();
}

function resizeCanvases() {
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  for (const canvas of [el.canvas, el.overlay]) {
    const width = Math.max(1, Math.round(canvas.clientWidth * dpr));
    const height = Math.max(1, Math.round(canvas.clientHeight * dpr));
    if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
  }
  draw();
}

function uploadNetwork(net) {
  const gl = state.gl;
  if (!gl || state.glNetwork === net) return;
  gl.bindBuffer(gl.ARRAY_BUFFER, state.glBuffers.pos);
  gl.bufferData(gl.ARRAY_BUFFER, net.positions, gl.STATIC_DRAW);
  gl.bindBuffer(gl.ARRAY_BUFFER, state.glBuffers.color);
  gl.bufferData(gl.ARRAY_BUFFER, net.colors, gl.STATIC_DRAW);
  state.glNetwork = net;
}

function drawNetwork() {
  const net = state.network, view = state.view;
  if (state.gl) {
    const gl = state.gl, dpr = el.canvas.width / Math.max(1, el.canvas.clientWidth);
    gl.viewport(0, 0, el.canvas.width, el.canvas.height);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (!net || !net.lines) return;
    gl.useProgram(state.glProgram);
    gl.uniform2f(state.glInfo.center, view.x, view.y);
    gl.uniform2f(state.glInfo.resolution, el.canvas.width, el.canvas.height);
    gl.uniform1f(state.glInfo.scale, view.scale * dpr);
    gl.uniform1f(state.glInfo.time, state.mode === "time" ? 1 : 0);
    gl.uniform3f(state.glInfo.base, 0.34, 0.43, 0.47);
    gl.bindBuffer(gl.ARRAY_BUFFER, state.glBuffers.pos);
    gl.enableVertexAttribArray(state.glInfo.pos);
    gl.vertexAttribPointer(state.glInfo.pos, 2, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, state.glBuffers.color);
    gl.enableVertexAttribArray(state.glInfo.color);
    gl.vertexAttribPointer(state.glInfo.color, 3, gl.FLOAT, false, 0, 0);
    gl.drawArrays(gl.LINES, 0, net.lines * 2);
    return;
  }
  const ctx = state.fallback;
  if (!ctx) return;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, el.canvas.width, el.canvas.height);
  if (!net || !net.lines) return;
  if (!net.paths2d) {
    const paths = Array.from({ length: 12 }, () => new Path2D());
    for (let i = 0; i < net.positions.length; i += 4) {
      const path = paths[net.bins[i / 4]];
      path.moveTo(net.positions[i], net.positions[i + 1]);
      path.lineTo(net.positions[i + 2], net.positions[i + 3]);
    }
    net.paths2d = paths;
  }
  const dpr = el.canvas.width / Math.max(1, el.canvas.clientWidth), s = view.scale * dpr;
  ctx.setTransform(s, 0, 0, -s, el.canvas.width / 2 - view.x * s, el.canvas.height / 2 + view.y * s);
  ctx.globalAlpha = 0.88; ctx.lineWidth = 0.75 / Math.max(0.01, view.scale);
  for (let i = 0; i < net.paths2d.length; i++) {
    const q = i / 11, red = Math.round(49 + (227 - 49) * q), green = Math.round(140 + (166 - 140) * q), blue = Math.round(135 + (77 - 135) * q);
    ctx.strokeStyle = state.mode === "time" ? `rgb(${red} ${green} ${blue})` : "#71838a";
    ctx.stroke(net.paths2d[i]);
  }
  ctx.globalAlpha = 1;
}

function worldToScreen(x, y, width, height) {
  return [width / 2 + (x - state.view.x) * state.view.scale, height / 2 - (y - state.view.y) * state.view.scale];
}

function drawOverlay() {
  const canvas = el.overlay, ctx = canvas.getContext("2d");
  const dpr = canvas.width / Math.max(1, canvas.clientWidth), width = canvas.clientWidth, height = canvas.clientHeight;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  if (!state.network) { el.timeLegend.hidden = true; return; }
  const net = state.network;
  el.timeLegend.hidden = state.mode !== "time" || !Number.isFinite(net.minSeconds);
  if (!el.timeLegend.hidden) {
    const fmt = (value) => new Intl.NumberFormat(undefined, { maximumFractionDigits: value < 10 ? 2 : 1 }).format(value);
    el.timeRange.textContent = `${fmt(net.minSeconds)} s  —  ${fmt(net.maxSeconds)} s`;
  }

  const step = niceStep(Math.max(width, height) / state.view.scale / 6);
  const left = state.view.x - width / (2 * state.view.scale), right = state.view.x + width / (2 * state.view.scale);
  const bottom = state.view.y - height / (2 * state.view.scale), top = state.view.y + height / (2 * state.view.scale);
  ctx.strokeStyle = "rgba(132,159,165,.12)"; ctx.lineWidth = 1;
  ctx.beginPath();
  for (let x = Math.ceil(left / step) * step; x <= right; x += step) {
    const sx = worldToScreen(x, 0, width, height)[0]; ctx.moveTo(sx, 0); ctx.lineTo(sx, height);
  }
  for (let y = Math.ceil(bottom / step) * step; y <= top; y += step) {
    const sy = worldToScreen(0, y, width, height)[1]; ctx.moveTo(0, sy); ctx.lineTo(width, sy);
  }
  ctx.stroke();

  const route = state.route;
  if (route && route.path && route.path.length) {
    const points = state.network.points;
    ctx.beginPath();
    route.path.forEach((node, i) => {
      if (node < 0 || node >= state.network.n) return;
      const [x, y] = worldToScreen(points[node * 2], points[node * 2 + 1], width, height);
      if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    });
    ctx.strokeStyle = "rgba(226,162,74,.22)"; ctx.lineWidth = 8; ctx.lineCap = "round"; ctx.lineJoin = "round"; ctx.stroke();
    ctx.strokeStyle = "#e2a24a"; ctx.lineWidth = 2.7; ctx.stroke();
  }
  drawMarker(ctx, el.origin.value, "#66c5b4", "A", width, height);
  drawMarker(ctx, el.destination.value, "#e2a24a", "B", width, height);
  drawScale(width, height);
}

function drawMarker(ctx, value, color, label, width, height) {
  if (!state.network || value === "") return;
  const node = Number(value);
  if (!Number.isInteger(node) || node < 0 || node >= state.network.n) return;
  const [x, y] = worldToScreen(state.network.points[2 * node], state.network.points[2 * node + 1], width, height);
  ctx.beginPath(); ctx.arc(x, y, 8, 0, Math.PI * 2); ctx.fillStyle = "#0b1921"; ctx.fill();
  ctx.beginPath(); ctx.arc(x, y, 5, 0, Math.PI * 2); ctx.fillStyle = color; ctx.fill();
  ctx.font = "600 11px Segoe UI, Arial, sans-serif"; ctx.textBaseline = "middle";
  const tx = x + 12, ty = y - 1;
  ctx.fillStyle = "rgba(8,19,25,.86)"; ctx.fillRect(tx - 4, ty - 9, 18, 18);
  ctx.fillStyle = color; ctx.fillText(label, tx, ty);
}

function drawScale(width, height) {
  const km = niceStep(76 / state.view.scale);
  const bar = Math.max(28, km * state.view.scale);
  el.scaleBar.style.setProperty("--scale-width", `${bar}px`);
  el.scaleBar.textContent = `${km < 10 ? km.toFixed(1) : km.toFixed(0)} km`;
}

function draw() {
  drawNetwork();
  drawOverlay();
}

function zoomAt(factor, px = el.stage.clientWidth / 2, py = el.stage.clientHeight / 2) {
  const view = state.view, width = el.stage.clientWidth, height = el.stage.clientHeight;
  const x = view.x + (px - width / 2) / view.scale;
  const y = view.y - (py - height / 2) / view.scale;
  view.scale = Math.min(5000, Math.max(0.0005, view.scale * factor));
  view.x = x - (px - width / 2) / view.scale;
  view.y = y + (py - height / 2) / view.scale;
  draw();
}

function closestNode(px, py) {
  const net = state.network;
  if (!net || !net.n) return null;
  const x = state.view.x + (px - el.stage.clientWidth / 2) / state.view.scale;
  const y = state.view.y - (py - el.stage.clientHeight / 2) / state.view.scale;
  let best = -1, distance = Infinity;
  for (let i = 0; i < net.n; i++) {
    const dx = net.points[2 * i] - x, dy = net.points[2 * i + 1] - y, d = dx * dx + dy * dy;
    if (d < distance) { best = i; distance = d; }
  }
  return best;
}

function clearRoute(message = "Choose two node IDs to see their exact route.") {
  state.routeVersion++;
  state.route = null;
  el.routeResult.replaceChildren();
  const p = document.createElement("p"); p.className = "result-placeholder"; p.textContent = message;
  el.routeResult.append(p);
  el.clear.disabled = true;
  draw();
}

function setEndpoint(which, value) {
  $(which === "origin" ? "origin-input" : "destination-input").value = String(value);
  el.pick.value = which === "origin" ? "destination" : "origin";
  clearRoute("Endpoints changed. Find the route to compare this pair.");
}

function formatDistance(result) {
  if (result.distance === null || result.distance === undefined) return "No route between these nodes.";
  const seconds = result.units === "milliseconds" ? result.distance / 1000 : result.distance;
  if (!Number.isFinite(seconds)) return "No route between these nodes.";
  return `${new Intl.NumberFormat(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 3 }).format(seconds)} s`;
}

async function findRoute(event) {
  event.preventDefault();
  if (!state.city || !state.network) return;
  const s = Number(el.origin.value), t = Number(el.destination.value);
  if (!Number.isInteger(s) || !Number.isInteger(t) || s < 0 || t < 0 || s >= state.network.n || t >= state.network.n) {
    renderRouteError(`Enter node IDs from 0 to ${Math.max(0, state.network.n - 1)}.`); return;
  }
  if (state.routeController) state.routeController.abort();
  const controller = new AbortController(); state.routeController = controller;
  const version = ++state.routeVersion, cityVersion = state.version;
  state.route = null; draw();
  el.routeResult.replaceChildren();
  const pending = document.createElement("p"); pending.className = "result-placeholder"; pending.textContent = "Computing exact route…";
  el.routeResult.append(pending);
  try {
    const url = `/api/route?city=${encodeURIComponent(state.city.id)}&source=${s}&target=${t}&scale=${metricScale()}`;
    const response = await get(url, { signal: controller.signal });
    const result = await response.json();
    if (version !== state.routeVersion || cityVersion !== state.version) return;
    if (result.source !== s || result.target !== t || result.scale !== metricScale()) throw new Error("Route response does not match the selected query.");
    state.route = result;
    renderRoute(result);
  } catch (error) {
    if (error.name !== "AbortError" && version === state.routeVersion) renderRouteError(`Route unavailable: ${error.message}`);
  }
}

function renderRouteError(message) {
  state.route = null; draw();
  el.routeResult.replaceChildren();
  const p = document.createElement("p"); p.className = "route-error"; p.textContent = message;
  el.routeResult.append(p); el.clear.disabled = false;
}

function renderRoute(result) {
  el.routeResult.replaceChildren();
  const distance = document.createElement("p"); distance.className = "route-distance"; distance.textContent = formatDistance(result);
  const meta = document.createElement("p"); meta.className = "route-meta";
  const count = Array.isArray(result.path) ? result.path.length : 0;
  const metric = result.units === "milliseconds" ? "rounded to milliseconds" : "source travel time";
  meta.textContent = result.distance === null
    ? `${result.certified === true ? "Certified unreachable result" : "Unreachable result"} · ${metric}`
    : `${result.certified === true ? "Certified exact path" : "Certification unavailable"} · ${count.toLocaleString()} nodes · ${metric}`;
  el.routeResult.append(distance, meta);
  el.clear.disabled = false;
  draw();
}

function metricScale() { return Number(document.querySelector('.metric-switch button[aria-pressed="true"]')?.dataset.scale || 0); }

function renderBenchRows() {
  const data = state.benchmark;
  el.benchRows.replaceChildren();
  if (!data || !Array.isArray(data.rows) || !data.rows.length) {
    const p = document.createElement("p"); p.className = "benchmark-empty"; p.textContent = "No measurements for this city and metric.";
    el.benchRows.append(p); return;
  }
  const stat = el.stat.value, rows = data.rows.filter((row) => Number.isFinite(Number(row[stat])))
    .slice().sort((a, b) => Number(a[stat]) - Number(b[stat]));
  const max = Math.max(1, ...rows.map((row) => Number(row[stat])));
  for (const row of rows) {
    const item = document.createElement("div"); item.className = `bench-row${row.method === "fast_paths-CH" ? " ch-row" : ""}`;
    const label = document.createElement("span"); label.className = "bench-label"; label.textContent = methodLabel(row.method); label.title = label.textContent;
    const value = document.createElement("span"); value.className = "bench-value"; value.textContent = `${Number(row[stat]).toLocaleString(undefined, { maximumFractionDigits: 1 })} µs`;
    const track = document.createElement("span"); track.className = "bench-track"; track.setAttribute("aria-hidden", "true");
    const bar = document.createElement("span"); bar.className = "bench-bar"; bar.style.width = `${Math.max(1, 100 * Number(row[stat]) / max)}%`; track.append(bar);
    item.append(label, value, track);
    if (Number(row.wrong) > 0) {
      const wrong = document.createElement("span"); wrong.className = "bench-extra"; wrong.textContent = `${row.wrong} mismatched answers`; item.append(wrong);
    }
    if (metricScale() === 1000 && Number.isFinite(Number(row.paired_ratio_to_ch))) {
      const ratio = document.createElement("span"); ratio.className = "bench-extra"; ratio.textContent = `Median paired ratio to CH: ${Number(row.paired_ratio_to_ch).toFixed(2)}×`; item.append(ratio);
    }
    if (row.method === "fast_paths-CH") {
      const note = document.createElement("span"); note.className = "bench-extra"; note.textContent = "Query timing includes path reconstruction."; item.append(note);
    }
    el.benchRows.append(item);
  }
}

function humanLabel(key) { return String(key).replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase()); }

function methodLabel(method) {
  if (method === "dijkstra") return "Dijkstra";
  if (method === "bidirectional") return "Bidirectional Dijkstra";
  if (method === "fast_paths-CH") return "Contraction hierarchy · CH";
  const match = /^(Graph|Hypergraph)-(\d+)$/.exec(method || "");
  return match ? `${match[1]} overlay · U=${match[2]}` : method || "Method";
}

function renderBuilds() {
  const data = state.benchmark;
  el.buildRows.replaceChildren();
  const rows = Array.isArray(data?.builds) ? data.builds : [];
  if (!rows.length) {
    const line = document.createElement("p"); line.className = "fine-print"; line.textContent = "No preprocessing measurements for this selection.";
    el.buildRows.append(line); return;
  }
  for (const row of rows) {
    const item = document.createElement("div"); item.className = "build-row";
    const name = document.createElement("span"); name.textContent = methodLabel(row.method);
    const values = [];
    const summary = document.createElement("span");
    if (Number.isFinite(row.serial_s)) values.push(`serial median ${row.serial_s.toFixed(3)} s`);
    if (Number.isFinite(row.parallel_s)) {
      values.push(`parallel median ${row.parallel_s.toFixed(3)} s`);
      if (row.parallel_s > 0 && Number.isFinite(row.serial_s)) values.push(`serial/parallel ${(row.serial_s / row.parallel_s).toFixed(3)}×`);
    }
    for (const key of ["boundary", "arcs"]) if (Number.isFinite(row[key])) values.push(`${humanLabel(key).toLowerCase()} ${Number(row[key]).toLocaleString()}`);
    if (row.query_includes_path_reconstruction === true) values.push("query includes path reconstruction");
    summary.textContent = values.join(" · ") || "—";
    item.append(name, summary); el.buildRows.append(item);
  }
}

function renderBenchmark(data) {
  state.benchmark = data;
  const graph = data.graph;
  if (graph) {
    const pairs = Array.isArray(graph.pairs) ? graph.pairs : [];
    const certified = Number(graph.certified_queries ?? graph.queries ?? pairs.length);
    el.benchContext.textContent = `${certified.toLocaleString()} certified query pairs · ${Number(graph.nodes).toLocaleString()} nodes · ${Number(graph.arcs).toLocaleString()} arcs`;
    fillPresets(pairs);
  } else {
    el.benchContext.textContent = "No benchmark run recorded for this city and metric.";
    fillPresets([]);
  }
  renderBenchRows(); renderBuilds();
  const machine = data.machine;
  el.machine.textContent = machine
    ? `CPU: ${machine.cpu || "not reported"}${Number.isFinite(Number(machine.os_cpu_count)) ? ` · ${machine.os_cpu_count} logical CPUs` : ""}`
    : "";
}

function fillPresets(pairs) {
  el.preset.replaceChildren();
  const first = document.createElement("option"); first.value = ""; first.textContent = pairs.length ? "Choose a measured pair" : "No measured pairs";
  el.preset.append(first);
  const indices = [0, 1, 2, 3, 125, 126, 501, 502, 750, 999].filter((i) => i < pairs.length);
  for (const i of indices) {
    const pair = pairs[i], source = Array.isArray(pair) ? pair[0] : pair.source, target = Array.isArray(pair) ? pair[1] : pair.target;
    if (!Number.isInteger(Number(source)) || !Number.isInteger(Number(target))) continue;
    const option = document.createElement("option"); option.value = String(i); option.textContent = `Pair ${i + 1} · ${source} → ${target}`; el.preset.append(option);
  }
  el.preset.disabled = indices.length === 0;
  el.preset.dataset.pairs = JSON.stringify(pairs);
}

async function loadBenchmark(cityVersion = state.version) {
  if (!state.city) return;
  if (state.benchController) state.benchController.abort();
  const controller = new AbortController(); state.benchController = controller;
  const version = ++state.benchVersion, scale = metricScale();
  state.benchmark = null;
  el.benchContext.textContent = "Loading measurements…";
  el.benchRows.replaceChildren();
  el.benchRows.append(Object.assign(document.createElement("p"), { className: "benchmark-empty", textContent: "Fetching measured runs…" }));
  el.buildRows.replaceChildren(); el.machine.textContent = "";
  el.preset.replaceChildren(Object.assign(document.createElement("option"), { value: "", textContent: "Loading measured pairs…" }));
  el.preset.disabled = true;
  try {
    const url = `/api/benchmark?city=${encodeURIComponent(state.city.id)}&scale=${scale}`;
    const response = await get(url, { signal: controller.signal });
    const data = await response.json();
    if (version !== state.benchVersion || cityVersion !== state.version || scale !== metricScale()) return;
    renderBenchmark(data);
  } catch (error) {
    if (error.name === "AbortError" || version !== state.benchVersion) return;
    el.benchContext.textContent = `Measurements unavailable: ${error.message}`;
    el.benchRows.replaceChildren();
    const retry = document.createElement("button"); retry.className = "text-button"; retry.type = "button"; retry.textContent = "Retry measurements";
    retry.addEventListener("click", () => loadBenchmark()); el.benchRows.append(retry);
    renderBuilds();
  }
}

async function loadCity() {
  const city = state.cities.find((item) => item.id === el.city.value) || state.city;
  if (!city) return;
  const changed = state.city?.id !== city.id;
  state.city = city;
  const version = ++state.version;
  for (const key of ["networkController", "benchController", "routeController"]) state[key]?.abort();
  state.network = null; state.benchmark = null; state.route = null;
  const shortLabel = city.label.replace(/,\s*large snapshot$/i, "");
  el.title.textContent = shortLabel;
  el.cityNote.textContent = city.id.endsWith("-large")
    ? `${city.country} · large network snapshot; not full-city coverage.`
    : `${city.country} · regional road graph; node geometry, not a street basemap.`;
  el.source.href = city.source_url || "https://huggingface.co/datasets/HBNU-NLP/InstructOSM";
  el.source.hidden = false;
  el.routeButton.disabled = true; el.preset.replaceChildren(); el.preset.disabled = true;
  if (changed) {
    el.origin.value = ""; el.destination.value = ""; el.pick.value = "origin";
  }
  clearRoute("Loading this graph…");
  setMapState(`Loading ${shortLabel} road graph…`, "loading");
  const controller = new AbortController(); state.networkController = controller;
  loadBenchmark(version);
  try {
    const response = await get(`/api/network?city=${encodeURIComponent(city.id)}`, { signal: controller.signal });
    const net = parseNetwork(await response.arrayBuffer(), city);
    if (version !== state.version) return;
    state.network = net;
    uploadNetwork(net);
    if (city.id.endsWith("-large")) {
      el.cityNote.textContent = `${city.country} · InstructOSM snapshot · ${net.n.toLocaleString()} nodes`;
    } else {
      el.cityNote.textContent = `${city.country} · regional graph · ${net.n.toLocaleString()} nodes.`;
    }
    el.routeButton.disabled = false;
    el.origin.max = String(Math.max(0, net.n - 1)); el.destination.max = el.origin.max;
    if (Number(el.origin.value) >= net.n) el.origin.value = "";
    if (Number(el.destination.value) >= net.n) el.destination.value = "";
    clearRoute("Pick endpoints or load a measured pair, then find the route.");
    setMapState(""); fitGraph();
  } catch (error) {
    if (error.name === "AbortError" || version !== state.version) return;
    setMapState(`${shortLabel} graph unavailable: ${error.message}`, "error", true);
  }
}

async function loadCities() {
  try {
    const response = await get("/api/cities");
    const data = await response.json();
    if (!Array.isArray(data.cities) || !data.cities.length) throw new Error("No city list was returned.");
    state.cities = data.cities;
    el.city.replaceChildren();
    for (const city of state.cities) {
      const option = document.createElement("option"); option.value = city.id;
      option.textContent = `${city.label}${city.country ? ` · ${city.country}` : ""}${city.available === false ? " · not downloaded" : ""}`;
      el.city.append(option);
    }
    el.city.disabled = false;
    const completed = data.benchmark?.completed_cities, total = data.benchmark?.total_cities;
    el.coverage.textContent = Number.isFinite(Number(completed)) && Number.isFinite(Number(total))
      ? `Benchmark coverage: ${completed} of ${total} cities.` : "Benchmark coverage reported per city.";
    const chosen = state.cities.find((city) => city.id === "london-large" && city.available !== false)
      || state.cities.find((city) => city.available !== false) || state.cities[0];
    el.city.value = chosen.id;
    await loadCity();
  } catch (error) {
    el.city.disabled = true;
    el.city.replaceChildren(Object.assign(document.createElement("option"), { textContent: "City service unavailable" }));
    if (location.protocol === "file:") {
      setMapState("Run python scripts/serve_maps.py, then open http://127.0.0.1:8765/.", "error");
    } else {
      setMapState(`Could not load city list: ${error.message}`, "error", true);
    }
  }
}

el.routeForm.addEventListener("submit", findRoute);
el.clear.addEventListener("click", () => clearRoute());
el.swap.addEventListener("click", () => {
  [el.origin.value, el.destination.value] = [el.destination.value, el.origin.value];
  clearRoute("Endpoints swapped. Find the route to compare this pair.");
});
el.preset.addEventListener("change", () => {
  const index = Number(el.preset.value);
  if (!Number.isInteger(index) || !el.preset.value) return;
  const pairs = JSON.parse(el.preset.dataset.pairs || "[]"), pair = pairs[index];
  if (!pair) return;
  el.origin.value = String(Array.isArray(pair) ? pair[0] : pair.source);
  el.destination.value = String(Array.isArray(pair) ? pair[1] : pair.target);
  clearRoute("Measured endpoints loaded. Find the route to see the certified path.");
});
for (const button of document.querySelectorAll(".metric-switch button")) {
  button.addEventListener("click", () => {
    for (const other of document.querySelectorAll(".metric-switch button")) other.setAttribute("aria-pressed", String(other === button));
    const rounded = metricScale() === 1000;
    el.metricNote.textContent = rounded
      ? "Each arc time is rounded to milliseconds, with a minimum of 1 ms."
      : "Native edge travel times, in seconds.";
    loadBenchmark();
    if (el.origin.value !== "" && el.destination.value !== "" && state.network) findRoute(new Event("submit", { cancelable: true }));
  });
}
el.stat.addEventListener("change", renderBenchRows);
el.city.addEventListener("change", loadCity);
$("edge-mode").addEventListener("change", (event) => {
  state.mode = event.currentTarget.value;
  draw();
});
$("zoom-in").addEventListener("click", () => zoomAt(1.35));
$("zoom-out").addEventListener("click", () => zoomAt(1 / 1.35));
$("fit-map").addEventListener("click", fitGraph);
el.canvas.addEventListener("wheel", (event) => {
  event.preventDefault();
  const rect = el.stage.getBoundingClientRect();
  zoomAt(event.deltaY < 0 ? 1.12 : 1 / 1.12, event.clientX - rect.left, event.clientY - rect.top);
}, { passive: false });
el.canvas.addEventListener("pointerdown", (event) => {
  el.canvas.setPointerCapture(event.pointerId);
  state.pointer = { x: event.clientX, y: event.clientY, moved: false, startX: event.clientX, startY: event.clientY };
});
el.canvas.addEventListener("pointermove", (event) => {
  const p = state.pointer;
  if (!p) return;
  const dx = event.clientX - p.x, dy = event.clientY - p.y;
  if (Math.abs(event.clientX - p.startX) + Math.abs(event.clientY - p.startY) > 4) p.moved = true;
  if (p.moved) {
    state.view.x -= dx / state.view.scale;
    state.view.y += dy / state.view.scale;
    draw();
  }
  p.x = event.clientX; p.y = event.clientY;
});
el.canvas.addEventListener("pointerup", (event) => {
  const p = state.pointer; state.pointer = null;
  if (!p || p.moved) return;
  const rect = el.stage.getBoundingClientRect(), node = closestNode(event.clientX - rect.left, event.clientY - rect.top);
  if (node !== null) setEndpoint(el.pick.value, node);
});
el.canvas.addEventListener("pointercancel", () => { state.pointer = null; });
el.origin.addEventListener("input", () => { clearRoute("Endpoints changed. Find the route to compare this pair."); });
el.destination.addEventListener("input", () => { clearRoute("Endpoints changed. Find the route to compare this pair."); });
window.addEventListener("keydown", (event) => {
  if (document.activeElement !== el.canvas) return;
  if (event.key === "+" || event.key === "=") { event.preventDefault(); zoomAt(1.25); }
  else if (event.key === "-") { event.preventDefault(); zoomAt(0.8); }
  else if (event.key === "0") { event.preventDefault(); fitGraph(); }
  else if (event.key === "ArrowLeft") { event.preventDefault(); state.view.x -= 40 / state.view.scale; draw(); }
  else if (event.key === "ArrowRight") { event.preventDefault(); state.view.x += 40 / state.view.scale; draw(); }
  else if (event.key === "ArrowUp") { event.preventDefault(); state.view.y += 40 / state.view.scale; draw(); }
  else if (event.key === "ArrowDown") { event.preventDefault(); state.view.y -= 40 / state.view.scale; draw(); }
});

initRenderer();
new ResizeObserver(() => { resizeCanvases(); if (state.network) fitGraph(); }).observe(el.stage);
resizeCanvases();
loadCities();
