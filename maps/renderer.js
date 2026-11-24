/* Actual graph geometry and certified query traces for the paired partition views. */
(() => {
  "use strict";

  const NAVY = "#10232c";
  const AMBER = [0.78, 0.52, 0.20];
  const BLUE = [0.24, 0.48, 0.69];
  const PALE = [0.75, 0.80, 0.79];
  const shaderSource = {
    vertex: `attribute vec2 a_pos; attribute vec3 a_color;
      uniform vec2 u_center; uniform vec2 u_resolution; uniform float u_scale; uniform float u_size;
      varying vec3 v_color;
      void main(){ vec2 p=(a_pos-u_center)*u_scale;
        gl_Position=vec4(2.0*p.x/u_resolution.x,2.0*p.y/u_resolution.y,0.0,1.0);
        gl_PointSize=u_size; v_color=a_color; }`,
    fragment: `precision mediump float; varying vec3 v_color;
      void main(){ gl_FragColor=vec4(v_color,1.0); }`,
  };

  function rgb(h, s, l) {
    const a = s * Math.min(l, 1 - l);
    const f = (n) => {
      const k = (n + h * 12) % 12;
      return l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    };
    return [f(0), f(8), f(4)];
  }

  function cellColor(id, model) {
    let x = (id + (model === "Graph" ? 0x9e3779b9 : 0x6d2b79f5)) >>> 0;
    x = Math.imul(x ^ (x >>> 16), 0x21f0aaad) >>> 0;
    x = Math.imul(x ^ (x >>> 15), 0x735a2d97) >>> 0;
    x = (x ^ (x >>> 15)) >>> 0;
    return rgb(x / 4294967296, 0.48, 0.53);
  }

  function compile(gl, type, source) {
    const shader = gl.createShader(type);
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader));
    return shader;
  }

  function finiteBounds(bounds) {
    return bounds && [bounds.minX, bounds.maxX, bounds.minY, bounds.maxY].every(Number.isFinite)
      && bounds.maxX >= bounds.minX && bounds.maxY >= bounds.minY;
  }

  class PartitionMaps {
    constructor() {
      this.layer = "boundary";
      this.network = null;
      this.diagnostic = null;
      this.query = null;
      this.camera = { x: 0, y: 0, scale: 1 };
      this.frame = 0;
      this.drag = null;
      this.panes = [
        this.makePane("graph-canvas", "graph-overlay", "Graph"),
        this.makePane("hypergraph-canvas", "hypergraph-overlay", "Hypergraph"),
      ];
      this.resizeObserver = new ResizeObserver(() => {
        this.resize();
        this.fit();
      });
      for (const pane of this.panes) this.resizeObserver.observe(pane.canvas.parentElement);
      this.resize();
    }

    makePane(canvasId, overlayId, model) {
      const canvas = document.getElementById(canvasId);
      const overlay = document.getElementById(overlayId);
      if (!canvas || !overlay) throw new Error(`Missing renderer canvases: ${canvasId}, ${overlayId}`);
      const pane = { canvas, overlay, model, gl: null, fallback: null, buffers: null, paths: null, data: null };
      try {
        const gl = canvas.getContext("webgl", { alpha: false, antialias: true, powerPreference: "high-performance" });
        if (gl) {
          const program = gl.createProgram();
          gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, shaderSource.vertex));
          gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, shaderSource.fragment));
          gl.linkProgram(program);
          if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
          pane.gl = gl;
          pane.data = {
            program,
            pos: gl.getAttribLocation(program, "a_pos"), color: gl.getAttribLocation(program, "a_color"),
            center: gl.getUniformLocation(program, "u_center"), resolution: gl.getUniformLocation(program, "u_resolution"),
            scale: gl.getUniformLocation(program, "u_scale"), size: gl.getUniformLocation(program, "u_size"),
          };
          pane.buffers = {};
          gl.clearColor(16 / 255, 35 / 255, 44 / 255, 1);
          gl.enable(gl.BLEND);
          gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
        } else {
          pane.fallback = canvas.getContext("2d");
          if (!pane.fallback) throw new Error("WebGL and Canvas rendering are unavailable.");
        }
      } catch (error) {
        pane.gl = null;
        pane.fallback = canvas.getContext("2d");
        if (!pane.fallback) throw error;
      }
      canvas.addEventListener("wheel", (event) => {
        event.preventDefault();
        const rect = canvas.getBoundingClientRect();
        this.zoom(Math.exp(-event.deltaY * 0.0012), event.clientX - rect.left, event.clientY - rect.top, pane);
      }, { passive: false });
      canvas.addEventListener("pointerdown", (event) => {
        canvas.focus({ preventScroll: true });
        canvas.setPointerCapture(event.pointerId);
        this.drag = { id: event.pointerId, x: event.clientX, y: event.clientY };
      });
      canvas.addEventListener("pointermove", (event) => {
        if (!this.drag || this.drag.id !== event.pointerId) return;
        const dx = event.clientX - this.drag.x, dy = event.clientY - this.drag.y;
        this.drag = { id: event.pointerId, x: event.clientX, y: event.clientY };
        this.camera.x -= dx / this.camera.scale;
        this.camera.y += dy / this.camera.scale;
        this.schedule();
      });
      const endDrag = (event) => { if (this.drag?.id === event.pointerId) this.drag = null; };
      canvas.addEventListener("pointerup", endDrag);
      canvas.addEventListener("pointercancel", endDrag);
      canvas.addEventListener("keydown", (event) => {
        if (event.key === "+" || event.key === "=") { event.preventDefault(); this.zoom(1.25); }
        else if (event.key === "-") { event.preventDefault(); this.zoom(0.8); }
        else if (event.key === "0") { event.preventDefault(); this.fit(); }
        else if (event.key.startsWith("Arrow")) {
          event.preventDefault();
          const step = 36 / this.camera.scale;
          if (event.key === "ArrowLeft") this.camera.x -= step;
          if (event.key === "ArrowRight") this.camera.x += step;
          if (event.key === "ArrowUp") this.camera.y += step;
          if (event.key === "ArrowDown") this.camera.y -= step;
          this.schedule();
        }
      });
      return pane;
    }

    setData(network, diagnostic) {
      if (!network || !Number.isInteger(network.n) || !(network.points instanceof Float32Array)
        || network.points.length !== network.n * 2 || !finiteBounds(network.bounds)) {
        throw new Error("Network points or bounds are invalid.");
      }
      if (!diagnostic || diagnostic.nodes !== network.n || !Array.isArray(diagnostic.tail)
        || !Array.isArray(diagnostic.head) || diagnostic.tail.length !== diagnostic.arcs
        || diagnostic.head.length !== diagnostic.arcs) throw new Error("Partition geometry does not match the network.");
      for (const model of ["Graph", "Hypergraph"]) {
        const item = diagnostic.models?.[model];
        if (!item || !Array.isArray(item.arc_cells) || item.arc_cells.length !== diagnostic.arcs
          || !Array.isArray(item.boundary_nodes)) throw new Error(`Missing ${model} partition geometry.`);
      }
      this.network = network;
      this.diagnostic = diagnostic;
      this.query = null;
      for (const pane of this.panes) {
        this.disposePane(pane);
        this.buildPane(pane);
      }
      this.fit();
    }

    buildPane(pane) {
      const n = this.network.n, d = this.diagnostic, part = d.models[pane.model];
      const cellBuffers = new Map();
      const positions = pane.gl ? new Float32Array(d.arcs * 4) : null;
      const colors = pane.gl ? new Float32Array(d.arcs * 6) : null;
      for (let a = 0; a < d.arcs; a++) {
        const u = d.tail[a], v = d.head[a];
        if (!Number.isInteger(u) || !Number.isInteger(v) || u < 0 || v < 0 || u >= n || v >= n) {
          throw new Error(`Arc ${a} has a node outside the graph.`);
        }
        const colorId = part.arc_cells[a];
        if (colorId !== null && (!Number.isInteger(colorId) || colorId < 0 || colorId >= part.cells)) {
          throw new Error(`Arc ${a} has an invalid cell ID.`);
        }
        const color = colorId === null ? AMBER : cellColor(colorId, pane.model);
        const key = colorId === null ? "cut" : colorId;
        const x1 = this.network.points[2 * u], y1 = this.network.points[2 * u + 1];
        const x2 = this.network.points[2 * v], y2 = this.network.points[2 * v + 1];
        if (pane.gl) {
          const p = 4 * a, c = 6 * a;
          positions[p] = x1; positions[p + 1] = y1; positions[p + 2] = x2; positions[p + 3] = y2;
          colors[c] = color[0]; colors[c + 1] = color[1]; colors[c + 2] = color[2];
          colors[c + 3] = color[0]; colors[c + 4] = color[1]; colors[c + 5] = color[2];
        } else {
          let group = cellBuffers.get(key);
          if (!group) { group = { color, path: new Path2D() }; cellBuffers.set(key, group); }
          group.path.moveTo(x1, y1);
          group.path.lineTo(x2, y2);
        }
      }
      const roadsGray = pane.fallback ? new Path2D() : null;
      if (roadsGray) {
        for (let a = 0; a < d.arcs; a++) {
          const u = d.tail[a], v = d.head[a];
          roadsGray.moveTo(this.network.points[2 * u], this.network.points[2 * u + 1]);
          roadsGray.lineTo(this.network.points[2 * v], this.network.points[2 * v + 1]);
        }
      }
      const boundaries = part.boundary_nodes;
      for (const v of boundaries) if (!Number.isInteger(v) || v < 0 || v >= n) throw new Error("Boundary node outside graph.");
      pane.static = {
        cellBuffers,
        roadsGray,
        line: pane.gl ? this.makeBuffer(pane, positions, colors) : null,
        boundary: this.pointsBuffer(pane, boundaries, AMBER),
        boundaryNodes: boundaries,
        boundarySet: new Set(boundaries),
      };
      pane.search = null;
    }

    disposePane(pane) {
      if (!pane.gl) return;
      const gl = pane.gl;
      const drop = (buffer) => {
        if (buffer) { gl.deleteBuffer(buffer.pos); gl.deleteBuffer(buffer.color); }
      };
      if (pane.static) {
        for (const item of pane.static.cellBuffers.values()) drop(item.buffer);
        drop(pane.static.line); drop(pane.static.boundary);
      }
      if (pane.search) { drop(pane.search.interior); drop(pane.search.boundary); }
      pane.static = null; pane.search = null;
    }

    makeBuffer(pane, positions, colors) {
      if (!pane.gl) return { count: positions.length / 2, positions, colors };
      const gl = pane.gl;
      const pos = gl.createBuffer(), color = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, pos); gl.bufferData(gl.ARRAY_BUFFER, positions, gl.STATIC_DRAW);
      gl.bindBuffer(gl.ARRAY_BUFFER, color); gl.bufferData(gl.ARRAY_BUFFER, colors, gl.STATIC_DRAW);
      return { pos, color, count: positions.length / 2 };
    }

    pointsBuffer(pane, nodes, color) {
      if (!pane.gl) return { count: nodes.length };
      const positions = new Float32Array(nodes.length * 2), colors = new Float32Array(nodes.length * 3);
      for (let i = 0; i < nodes.length; i++) {
        const v = nodes[i], p = 2 * i, c = 3 * i;
        positions[p] = this.network.points[2 * v]; positions[p + 1] = this.network.points[2 * v + 1];
        colors[c] = color[0]; colors[c + 1] = color[1]; colors[c + 2] = color[2];
      }
      return this.makeBuffer(pane, positions, colors);
    }

    setLayer(layer) {
      if (layer !== "boundary" && layer !== "search") throw new Error("Layer must be boundary or search.");
      this.layer = layer;
      this.schedule();
    }

    setQuery(query) {
      this.query = query || null;
      if (this.network && this.query) {
        for (const pane of this.panes) {
          if (pane.gl && pane.search) {
            pane.gl.deleteBuffer(pane.search.interior.pos); pane.gl.deleteBuffer(pane.search.interior.color);
            pane.gl.deleteBuffer(pane.search.boundary.pos); pane.gl.deleteBuffer(pane.search.boundary.color);
          }
          const part = this.diagnostic.models[pane.model];
          const boundary = new Set(part.boundary_nodes);
          const settled = pane.model === "Graph" ? query.Graph?.settled_nodes : query.Hypergraph?.settled_nodes;
          const interior = [], edge = [];
          for (const v of settled || []) (boundary.has(v) ? edge : interior).push(v);
          pane.search = {
            interior: this.pointsBuffer(pane, interior, BLUE),
            boundary: this.pointsBuffer(pane, edge, AMBER),
            interiorNodes: interior,
            boundaryNodes: edge,
          };
        }
      } else for (const pane of this.panes) pane.search = null;
      this.schedule();
    }

    clear() {
      this.network = null;
      this.diagnostic = null;
      this.query = null;
      for (const pane of this.panes) {
        this.disposePane(pane); pane.paths = null;
        pane.overlay.getContext("2d")?.clearRect(0, 0, pane.overlay.width, pane.overlay.height);
        if (pane.gl) { pane.gl.clearColor(16 / 255, 35 / 255, 44 / 255, 1); pane.gl.clear(pane.gl.COLOR_BUFFER_BIT); }
        else pane.fallback?.clearRect(0, 0, pane.canvas.width, pane.canvas.height);
      }
    }

    fit() {
      if (!this.network) { this.schedule(); return; }
      const rects = this.panes.map(p => p.canvas.getBoundingClientRect());
      const width = Math.min(...rects.map(r => r.width)), height = Math.min(...rects.map(r => r.height));
      const b = this.network.bounds;
      const spanX = Math.max(b.maxX - b.minX, 0.02), spanY = Math.max(b.maxY - b.minY, 0.02);
      this.camera = { x: (b.minX + b.maxX) / 2, y: (b.minY + b.maxY) / 2,
        scale: Math.max(0.001, Math.min((width - 52) / spanX, (height - 52) / spanY)) };
      this.schedule();
    }

    zoom(factor, x, y, pane) {
      const scale = this.camera.scale;
      if (Number.isFinite(x) && Number.isFinite(y) && pane) {
        const rect = pane.canvas.getBoundingClientRect();
        const wx = this.camera.x + (x - rect.width / 2) / scale;
        const wy = this.camera.y - (y - rect.height / 2) / scale;
        this.camera.scale = Math.max(0.001, Math.min(1e7, scale * factor));
        this.camera.x = wx - (x - rect.width / 2) / this.camera.scale;
        this.camera.y = wy + (y - rect.height / 2) / this.camera.scale;
      } else this.camera.scale = Math.max(0.001, Math.min(1e7, scale * factor));
      this.schedule();
    }

    resize() {
      const dpr = Math.min(2, window.devicePixelRatio || 1);
      for (const pane of this.panes) for (const canvas of [pane.canvas, pane.overlay]) {
        const w = Math.max(1, Math.round(canvas.clientWidth * dpr));
        const h = Math.max(1, Math.round(canvas.clientHeight * dpr));
        if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
      }
      this.schedule();
    }

    schedule() {
      if (this.frame) return;
      this.frame = requestAnimationFrame(() => { this.frame = 0; this.draw(); });
    }

    drawBuffer(pane, buffer, primitive, size, colorOverride = null) {
      if (!buffer || !buffer.count) return;
      const gl = pane.gl, info = pane.data;
      gl.useProgram(info.program);
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer.pos);
      gl.enableVertexAttribArray(info.pos); gl.vertexAttribPointer(info.pos, 2, gl.FLOAT, false, 0, 0);
      if (colorOverride) {
        gl.disableVertexAttribArray(info.color);
        gl.vertexAttrib3f(info.color, ...colorOverride);
      } else {
        gl.bindBuffer(gl.ARRAY_BUFFER, buffer.color);
        gl.enableVertexAttribArray(info.color); gl.vertexAttribPointer(info.color, 3, gl.FLOAT, false, 0, 0);
      }
      gl.uniform2f(info.center, this.camera.x, this.camera.y);
      gl.uniform2f(info.resolution, pane.canvas.width, pane.canvas.height);
      gl.uniform1f(info.scale, this.camera.scale * pane.canvas.width / pane.canvas.clientWidth);
      gl.uniform1f(info.size, size * pane.canvas.width / pane.canvas.clientWidth);
      gl.drawArrays(primitive, 0, buffer.count);
    }

    drawGl(pane) {
      const gl = pane.gl, dpr = pane.canvas.width / Math.max(1, pane.canvas.clientWidth);
      gl.viewport(0, 0, pane.canvas.width, pane.canvas.height);
      gl.clear(gl.COLOR_BUFFER_BIT);
      if (!pane.static || !this.network) return;
      if (this.layer === "boundary") {
        this.drawBuffer(pane, pane.static.line, gl.LINES, 1);
        this.drawBuffer(pane, pane.static.boundary, gl.POINTS, 3.2);
      } else {
        this.drawBuffer(pane, pane.static.line, gl.LINES, 1, [0.32, 0.42, 0.45]);
        if (pane.search) {
          this.drawBuffer(pane, pane.search.interior, gl.POINTS, 3.1);
          this.drawBuffer(pane, pane.search.boundary, gl.POINTS, 3.3);
        }
      }
      void dpr;
    }

    drawFallback(pane) {
      const ctx = pane.fallback, canvas = pane.canvas, dpr = canvas.width / Math.max(1, canvas.clientWidth);
      ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.fillStyle = NAVY; ctx.fillRect(0, 0, canvas.width, canvas.height);
      if (!pane.static || !this.network) return;
      ctx.setTransform(this.camera.scale * dpr, 0, 0, -this.camera.scale * dpr,
        dpr * (canvas.clientWidth / 2 - this.camera.x * this.camera.scale),
        dpr * (canvas.clientHeight / 2 + this.camera.y * this.camera.scale));
      ctx.lineWidth = 1 / this.camera.scale;
      if (this.layer === "boundary") {
        for (const item of pane.static.cellBuffers.values()) {
          const c = item.color.map(v => Math.round(v * 255));
          ctx.strokeStyle = `rgb(${c.join(",")})`; ctx.stroke(item.path);
        }
        this.drawFallbackPoints(ctx, pane.static.boundaryNodes, AMBER);
      } else {
        ctx.strokeStyle = "rgba(177,194,196,.25)"; ctx.stroke(pane.static.roadsGray);
        if (pane.search) {
          this.drawFallbackPoints(ctx, pane.search.interiorNodes, BLUE);
          this.drawFallbackPoints(ctx, pane.search.boundaryNodes, AMBER);
        }
      }
    }

    drawFallbackPoints(ctx, nodes, color) {
      ctx.fillStyle = `rgb(${color.map(v => Math.round(v * 255)).join(",")})`;
      const radius = 1.5 / this.camera.scale;
      ctx.beginPath();
      for (const v of nodes) {
        const x = this.network.points[2 * v], y = this.network.points[2 * v + 1];
        ctx.moveTo(x + radius, y); ctx.arc(x, y, radius, 0, Math.PI * 2);
      }
      ctx.fill();
    }

    drawOverlay(pane) {
      const canvas = pane.overlay, ctx = canvas.getContext("2d"), dpr = canvas.width / Math.max(1, canvas.clientWidth);
      const w = canvas.clientWidth, h = canvas.clientHeight;
      ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.clearRect(0, 0, canvas.width, canvas.height);
      if (!this.network) return;
      const xy = (v) => [w / 2 + (this.network.points[2 * v] - this.camera.x) * this.camera.scale,
        h / 2 - (this.network.points[2 * v + 1] - this.camera.y) * this.camera.scale];
      if (this.layer === "search" && this.query) {
        this.marker(ctx, xy(this.query.source), "S", dpr);
        this.marker(ctx, xy(this.query.target), "T", dpr);
      }
      this.scaleBar(ctx, w, h, dpr);
    }

    marker(ctx, [x, y], label, dpr) {
      ctx.save(); ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.beginPath(); ctx.arc(x, y, 5, 0, Math.PI * 2); ctx.fillStyle = "#f7f4e9"; ctx.fill();
      ctx.lineWidth = 1.5; ctx.strokeStyle = "#10232c"; ctx.stroke();
      ctx.font = "600 11px Segoe UI, Arial, sans-serif"; ctx.textBaseline = "middle";
      const tx = x + 8, tw = ctx.measureText(label).width;
      ctx.fillStyle = "rgba(9,22,29,.84)"; ctx.fillRect(tx - 3, y - 8, tw + 7, 16);
      ctx.fillStyle = "#f7f4e9"; ctx.fillText(label, tx, y);
      ctx.restore();
    }

    scaleBar(ctx, w, h, dpr) {
      if (!(this.camera.scale > 0)) return;
      const targetKm = 82 / this.camera.scale;
      const power = Math.pow(10, Math.floor(Math.log10(targetKm)));
      const ratio = targetKm / power;
      const factor = ratio >= 5 ? 5 : ratio >= 2 ? 2 : 1;
      const km = factor * power, px = km * this.camera.scale;
      ctx.save(); ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      const x = 18, y = h - 20;
      ctx.strokeStyle = "#d4dddb"; ctx.fillStyle = "#edf1e9"; ctx.lineWidth = 1.5;
      ctx.beginPath(); ctx.moveTo(x, y); ctx.lineTo(x + px, y); ctx.moveTo(x, y - 4); ctx.lineTo(x, y + 4);
      ctx.moveTo(x + px, y - 4); ctx.lineTo(x + px, y + 4); ctx.stroke();
      ctx.font = "11px Segoe UI, Arial, sans-serif"; ctx.textBaseline = "bottom";
      ctx.fillText(`${Number(km.toPrecision(3))} km`, x, y - 5);
      ctx.restore();
    }

    draw() {
      for (const pane of this.panes) {
        if (pane.gl) this.drawGl(pane);
        else if (pane.fallback) this.drawFallback(pane);
        this.drawOverlay(pane);
      }
    }
  }

  window.PartitionMaps = PartitionMaps;
})();
