const EMPTY = {
  generated_at: "",
  app: { status: "Ready", root: "" },
  top: { query: "", busy: false, watch_running: false, hits: 0, latency_ms: null },
  sidebar: {
    selected_filter: "recents",
    collections: [
      { key: "recents", label: "Recents", count: 0 },
      { key: "documents", label: "Documents", count: 0 },
      { key: "images", label: "Images", count: 0 },
      { key: "media", label: "Media", count: 0 },
      { key: "source", label: "Source Code", count: 0 },
      { key: "pdf", label: "PDF Files", count: 0 },
    ],
    show_snippets: true,
    regex: "",
    path_prefix: "",
    limit: 20,
  },
  results: { total_hits: 0, took_ms: null, items: [] },
  right_panel: {
    visible: true,
    tab: "preview",
    selected_path: null,
    file_name: null,
    file_type: null,
    size: null,
    created: null,
    modified: null,
    snippet: null,
    match_count: null,
    chat_mode: "extractive",
    chat_messages: [],
    chat_input: "",
    chat_busy: false,
  },
};

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "tif", "tiff"]);
const MEDIA_EXTS = new Set([
  "mp3",
  "wav",
  "flac",
  "ogg",
  "m4a",
  "aac",
  "mp4",
  "mkv",
  "mov",
  "avi",
  "wmv",
  "webm",
]);
const DOCUMENT_EXTS = new Set(["doc", "docx", "txt", "md", "rtf", "odt", "csv", "xls", "xlsx", "ppt", "pptx"]);
const SOURCE_EXTS = new Set([
  "rs",
  "js",
  "ts",
  "tsx",
  "jsx",
  "py",
  "java",
  "kt",
  "go",
  "c",
  "cpp",
  "h",
  "hpp",
  "cs",
  "php",
  "rb",
  "swift",
  "scala",
  "lua",
  "json",
  "yml",
  "yaml",
  "toml",
  "xml",
  "html",
  "css",
  "sql",
  "sh",
  "bat",
  "ps1",
]);

function classifyCollection(ext) {
  const e = String(ext || "").toLowerCase();
  if (e === "pdf") return "pdf";
  if (IMAGE_EXTS.has(e)) return "images";
  if (MEDIA_EXTS.has(e)) return "media";
  if (SOURCE_EXTS.has(e)) return "source";
  if (DOCUMENT_EXTS.has(e)) return "documents";
  return "documents";
}

function withCollectionCounts(items, selectedFilter) {
  const counts = {
    recents: items.length,
    documents: 0,
    images: 0,
    media: 0,
    source: 0,
    pdf: 0,
  };
  for (const it of items) {
    const k = classifyCollection(it.ext);
    counts[k] = (counts[k] || 0) + 1;
  }
  return [
    { key: "recents", label: "Recents", count: counts.recents },
    { key: "documents", label: "Documents", count: counts.documents },
    { key: "images", label: "Images", count: counts.images },
    { key: "media", label: "Media", count: counts.media },
    { key: "source", label: "Source Code", count: counts.source },
    { key: "pdf", label: "PDF Files", count: counts.pdf },
  ].map((c) => ({ ...c, active: c.key === selectedFilter }));
}

function itemMatchesCollection(item, key) {
  if (!key || key === "recents") return true;
  return classifyCollection(item.ext) === key;
}

function esc(v) {
  return String(v ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function extClass(ext) {
  const e = String(ext || "").toLowerCase();
  if (e === "pdf") return "pdf";
  if (["png", "jpg", "jpeg", "webp"].includes(e)) return "png";
  if (e === "skb") return "skb";
  if (e === "docx") return "docx";
  return "";
}

function extLabel(ext) {
  const e = String(ext || "").toLowerCase();
  if (e === "docx") return "W";
  if (e === "pdf") return "PDF";
  if (["png", "jpg", "jpeg", "webp"].includes(e)) return "IMG";
  if (e === "json") return "JSON";
  if (e === "txt") return "TXT";
  if (e === "rs") return "RS";
  if (e === "js" || e === "ts") return "JS";
  if (e === "skb") return "SKB";
  return (e || "FILE").slice(0, 4).toUpperCase();
}

function isImageExt(ext) {
  const e = String(ext || "").toLowerCase();
  return ["png", "jpg", "jpeg", "webp", "gif", "bmp", "ico", "tif", "tiff", "svg"].includes(e);
}

function fileSrc(path) {
  const raw = String(path || "");
  if (!raw) return "";
  try {
    const t = window.__TAURI__;
    if (t && typeof t.convertFileSrc === "function") {
      return t.convertFileSrc(raw);
    }
    if (t && t.tauri && typeof t.tauri.convertFileSrc === "function") {
      return t.tauri.convertFileSrc(raw);
    }
  } catch {
    // fall through
  }
  const normalized = raw.replace(/\\/g, "/");
  return encodeURI(`file:///${normalized}`);
}

function iconSrc(name) {
  return `./assets/icons/icon-${name}.svg`;
}

function iconImg(name) {
  return `<img class="icon-svg" src="${iconSrc(name)}" alt="" aria-hidden="true" />`;
}

function markSnippet(text, query) {
  if (!text) return "";
  const q = String(query || "").trim();
  if (!q) return esc(text);
  const safe = q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const parts = String(text).split(new RegExp(`(${safe})`, "ig"));
  return parts
    .map((p) =>
      p.toLowerCase() === q.toLowerCase() ? `<span class="mark">${esc(p)}</span>` : esc(p),
    )
    .join("");
}

function formatBytes(value) {
  const n = Number(value);
  if (!Number.isFinite(n) || n <= 0) return "-";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = n;
  let idx = 0;
  while (size >= 1024 && idx < units.length - 1) {
    size /= 1024;
    idx += 1;
  }
  const decimals = idx === 0 ? 0 : size < 10 ? 1 : 0;
  return `${size.toFixed(decimals)} ${units[idx]}`;
}

function mapSearchResult(query, res) {
  return {
    total_hits: res.total_hits || 0,
    took_ms: res.took_ms ?? null,
    items: (res.hits || []).map((h, idx) => ({
      rank: idx + 1,
      path: h.path,
      name: h.path.split(/[\\/]/).pop() || h.path,
      ext: (h.path.split(".").pop() || "").toLowerCase(),
      score: h.score || 0,
      selected: idx === 0,
      snippet: h.snippet || null,
      size: formatBytes(h.size_bytes),
      modified: h.modified || "-",
      created: h.created || "-",
    })),
    query,
  };
}

function effectiveLimit(state) {
  const raw = Number(state?.sidebar?.limit ?? 20);
  if (!Number.isFinite(raw) || raw <= 0) return 20;
  return Math.min(Math.max(Math.trunc(raw), 1), 200);
}

function tauriInvoke() {
  const api = window.__TAURI__;
  if (!api) return null;
  if (typeof api.invoke === "function") return api.invoke;
  if (api.tauri && typeof api.tauri.invoke === "function") return api.tauri.invoke;
  const internals = window.__TAURI_INTERNALS__;
  if (internals && typeof internals.invoke === "function") {
    return (cmd, payload) => internals.invoke(cmd, payload);
  }
  return null;
}

async function loadBridgeState() {
  try {
    const res = await fetch(`./state.json?t=${Date.now()}`, { cache: "no-store" });
    if (!res.ok) return EMPTY;
    return await res.json();
  } catch {
    return EMPTY;
  }
}

async function invokeDesktop(cmd, payload) {
  const invoke = tauriInvoke();
  if (!invoke) throw new Error("Desktop mode only");
  return invoke(cmd, payload);
}

class LupaShell extends HTMLElement {
  connectedCallback() {
    this.state = JSON.parse(JSON.stringify(EMPTY));
    this.monitorTimer = null;
    this._hotkeysBound = false;
    this._searchInFlight = false;
    this._pendingSearch = false;
    this._refreshTimer = null;
    this.mode = tauriInvoke() ? "tauri" : "bridge";
    requestAnimationFrame(() => this.paint());

    if (this.mode === "bridge") {
      this.timer = setInterval(async () => {
        const next = await loadBridgeState();
        if (JSON.stringify(next) !== JSON.stringify(this.state)) {
          this.state = next;
          this.paint();
        }
      }, 300);
    } else {
      this.initFromTauri().catch((err) => {
        this.state.app.status = `Init error: ${err}`;
        this.paint();
      });
    }
  }

  disconnectedCallback() {
    if (this.timer) clearInterval(this.timer);
    if (this.monitorTimer) clearInterval(this.monitorTimer);
    if (this._hotkeysBound) {
      window.removeEventListener("keydown", this._onKeyDown);
      this._hotkeysBound = false;
    }
  }

  async initFromTauri() {
    const invoke = tauriInvoke();
    if (!invoke) {
      this.state.app.status = "Desktop API not available (withGlobalTauri disabled?)";
      this.paint();
      return;
    }
    const boot = await invoke("bootstrap");
    this.state.app.root = (boot && boot.project_root) || "";
    this.state.app.status = "Desktop mode ready";
    this.paint();
  }

  paint() {
    const prevLeftHost = this.querySelector("lupa-left");
    const prevCenterHost = this.querySelector("lupa-center");
    const prevRightHost = this.querySelector("lupa-right");
    const scrollSnapshot = {
      left: prevLeftHost ? prevLeftHost.scrollTop : this._scrollLeft || 0,
      center: prevCenterHost ? prevCenterHost.scrollTop : this._scrollCenter || 0,
      right: prevRightHost ? prevRightHost.scrollTop : this._scrollRight || 0,
    };

    const s = this.state;
    const lat = s.results.took_ms == null ? "N/A" : `${s.results.took_ms}ms`;
    const stateText = s.top.busy ? "Indexing" : "Idle";
    const cpu = s.top.busy ? "busy" : "5%";
    const rightCol = s.right_panel.visible === false ? "0px" : "340px";
    const leftCol = "236px";

    this.innerHTML = `
      <div class="app-shell">
        <header class="topbar">
          <div class="brand">
            <div class="logo"></div>
            <div class="brand-text">LUPA</div>
          </div>
          <div class="v-divider"></div>
          <div class="search-wrap">
            <span class="search-glyph">Q</span>
            <input class="search-input" value="${esc(s.top.query || "")}" placeholder="Search files, docs, content..." />
            <div class="kbd-group">
              <span class="kbd-chip">Ctrl</span>
              <span class="kbd-chip">K</span>
            </div>
          </div>
          <button class="search-btn" id="btn-search">Search</button>
          <div class="state-pill"><span class="state-dot"></span>${stateText}</div>
          <div class="metrics">
            <span>FPS <span class="metric">N/A</span></span>
            <span class="sep"></span>
            <span>GPU <span class="metric">0%</span></span>
            <span class="sep"></span>
            <span>CPU <span class="w">${cpu}</span></span>
            <span class="sep"></span>
            <span>LAT <span class="metric">${lat}</span></span>
          </div>
        </header>
        <section class="main-grid" style="grid-template-columns: ${leftCol} 1fr ${rightCol};">
          <lupa-left></lupa-left>
          <lupa-center></lupa-center>
          ${s.right_panel.visible === false ? "<div></div>" : "<lupa-right></lupa-right>"}
        </section>
        <footer class="statusbar">
          <span>${esc(s.app.status || "Ready")}</span>
          <span>${s.results.total_hits} resultados en ${lat} | ${esc(s.app.root || "-")}</span>
        </footer>
      </div>
    `;

    const left = this.querySelector("lupa-left");
    const center = this.querySelector("lupa-center");
    const right = this.querySelector("lupa-right");
    if (left && typeof left.bind === "function") left.bind(this);
    if (center && typeof center.bind === "function") center.bind(this);
    if (right && typeof right.bind === "function") right.bind(this);

    if (left) {
      left.scrollTop = scrollSnapshot.left;
      this._scrollLeft = scrollSnapshot.left;
      left.addEventListener(
        "scroll",
        () => {
          this._scrollLeft = left.scrollTop;
        },
        { passive: true },
      );
    }
    if (center) {
      center.scrollTop = scrollSnapshot.center;
      this._scrollCenter = scrollSnapshot.center;
      center.addEventListener(
        "scroll",
        () => {
          this._scrollCenter = center.scrollTop;
        },
        { passive: true },
      );
    }
    if (right) {
      right.scrollTop = scrollSnapshot.right;
      this._scrollRight = scrollSnapshot.right;
      right.addEventListener(
        "scroll",
        () => {
          this._scrollRight = right.scrollTop;
        },
        { passive: true },
      );
    }
    if (this._ensureSelectedVisibleAfterPaint) {
      const activeRow = this.querySelector("lupa-center .row.active");
      if (activeRow && typeof activeRow.scrollIntoView === "function") {
        activeRow.scrollIntoView({ block: "nearest", inline: "nearest" });
      }
      if (center) {
        this._scrollCenter = center.scrollTop;
      }
      this._ensureSelectedVisibleAfterPaint = false;
    }

    const input = this.querySelector(".search-input");
    const btn = this.querySelector("#btn-search");
    if (input) {
      input.addEventListener("input", (ev) => {
        this.state.top.query = ev.target.value;
      });
      input.addEventListener("keydown", async (ev) => {
        if (ev.key === "Enter") await this.runSearch();
      });
    }
    if (btn) btn.addEventListener("click", async () => this.runSearch());

    if (!this._hotkeysBound) {
      this._onKeyDown = async (ev) => {
        const target = ev.target;
        const tag = target && target.tagName ? String(target.tagName).toLowerCase() : "";
        const isEditing = tag === "input" || tag === "textarea" || (target && target.isContentEditable);
        if (isEditing) return;

        if (ev.key === "Escape") {
          if (this.state.right_panel.visible !== false) {
            this.state.right_panel.visible = false;
            this.paint();
            ev.preventDefault();
          }
          return;
        }

        if (ev.key === "ArrowDown") {
          this.moveSelection(1);
          ev.preventDefault();
          return;
        }
        if (ev.key === "ArrowUp") {
          this.moveSelection(-1);
          ev.preventDefault();
          return;
        }
        if (ev.key === "Enter") {
          const selected = (this.state.results.items || []).find((it) => it.selected);
          if (selected) {
            await this.runPathAction("open", selected.path);
            ev.preventDefault();
          }
        }
      };
      window.addEventListener("keydown", this._onKeyDown);
      this._hotkeysBound = true;
    }
  }

  getFilteredIndexes() {
    const key = this.state.sidebar.selected_filter || "recents";
    const out = [];
    (this.state.results.items || []).forEach((it, idx) => {
      if (itemMatchesCollection(it, key)) out.push(idx);
    });
    return out;
  }

  moveSelection(delta) {
    const indexes = this.getFilteredIndexes();
    if (indexes.length === 0) return;
    const items = this.state.results.items || [];
    const current = items.findIndex((it) => it.selected);
    const currentPos = indexes.indexOf(current);
    const nextPos = currentPos < 0 ? 0 : Math.max(0, Math.min(indexes.length - 1, currentPos + delta));
    const nextIdx = indexes[nextPos];
    this.selectItem(nextIdx);
  }

  selectItem(idx, options = {}) {
    const keepViewport = options.keepViewport !== false;
    const items = this.state.results.items || [];
    if (!Number.isFinite(idx) || !items[idx]) return;
    items.forEach((it, i) => {
      it.selected = i === idx;
    });
    const item = items[idx];
    this.state.right_panel.visible = true;
    this.state.right_panel.selected_path = item.path;
    this.state.right_panel.file_name = item.name;
    this.state.right_panel.file_type = (item.ext || "").toUpperCase();
    this.state.right_panel.size = item.size || "-";
    this.state.right_panel.created = item.created || "-";
    this.state.right_panel.modified = item.modified || "-";
    this.state.right_panel.snippet = item.snippet || null;
    this.state.right_panel.match_count = item.snippet ? 1 : 0;
    this.state.right_panel.tab = "preview";
    this.state.right_panel.chat_input = "";
    this.state.right_panel.chat_busy = false;
    this.state.right_panel.chat_messages = [];
    if (keepViewport) {
      this._ensureSelectedVisibleAfterPaint = true;
    }
    this.paint();
  }

  selectFirstFromActiveCollection() {
    const key = this.state.sidebar.selected_filter || "recents";
    const items = this.state.results.items || [];
    const idx = items.findIndex((it) => itemMatchesCollection(it, key));
    if (idx >= 0) {
      this.selectItem(idx, { keepViewport: false });
      return;
    }
    items.forEach((it) => {
      it.selected = false;
    });
    this.state.right_panel.selected_path = null;
    this.state.right_panel.file_name = null;
    this.state.right_panel.file_type = null;
    this.state.right_panel.snippet = null;
    this.state.right_panel.match_count = 0;
    this.paint();
  }

  async runPathAction(action, path) {
    const safePath = String(path || "").trim();
    if (!safePath) {
      this.state.app.status = "No file selected";
      this.paint();
      return;
    }
    if (this.mode !== "tauri") {
      this.state.app.status = "Action requires desktop mode";
      this.paint();
      return;
    }
    try {
      if (action === "open") {
        await invokeDesktop("open_path", { req: { path: safePath } });
      } else if (action === "open_with") {
        await invokeDesktop("open_with", { req: { path: safePath } });
      } else if (action === "folder") {
        await invokeDesktop("open_folder", { req: { path: safePath } });
      } else if (action === "copy_path") {
        await invokeDesktop("copy_path", { req: { path: safePath } });
      } else if (action === "open_at_match") {
        await invokeDesktop("open_at_match", {
          req: { path: safePath, query: String(this.state.top.query || "") },
        });
      }
      this.state.app.status = `Action done: ${action}`;
    } catch (err) {
      this.state.app.status = `Action error: ${err}`;
    }
    this.paint();
  }

  async runSearch() {
    const query = (this.state.top.query || "").trim();
    if (!query) {
      this.state.app.status = "Type a query";
      this.paint();
      return;
    }
    if (this._searchInFlight) {
      this._pendingSearch = true;
      return;
    }

    this._searchInFlight = true;
    this.state.top.busy = true;
    this.state.app.status = "Searching...";
    this.paint();

    if (this.mode !== "tauri") {
      this._searchInFlight = false;
      this.state.top.busy = false;
      this.state.app.status = "Bridge mode: search runs only in desktop-tauri";
      this.paint();
      return;
    }

    try {
      const invoke = tauriInvoke();
      const res = await invoke("search", {
        req: {
          root: this.state.app.root || "",
          query,
          limit: this.state.sidebar.limit || 20,
          path_prefix: this.state.sidebar.path_prefix || null,
          regex: this.state.sidebar.regex || null,
          highlight: this.state.sidebar.show_snippets !== false,
        },
      });

      const mapped = mapSearchResult(query, res);
      this.state.results.total_hits = mapped.total_hits;
      this.state.results.took_ms = mapped.took_ms;
      this.state.results.items = mapped.items;
      this.state.results.visible_count = Math.min(mapped.items.length, effectiveLimit(this.state));
      this.state.top.latency_ms = mapped.took_ms;
      this.state.top.hits = mapped.total_hits;
      this.state.top.query = query;
      this.state.sidebar.collections = withCollectionCounts(
        mapped.items,
        this.state.sidebar.selected_filter || "recents",
      );
      this.state.app.status = `${mapped.total_hits} results`;
      this.selectFirstFromActiveCollection();
    } catch (err) {
      this.state.app.status = `Search error: ${err}`;
      this._searchInFlight = false;
      this.state.top.busy = false;
      this.paint();
      return;
    }
    this._searchInFlight = false;
    this.state.top.busy = false;
    this.paint();
    if (this._pendingSearch) {
      this._pendingSearch = false;
      this.runSearch();
    }
  }

  scheduleSearchRefresh(delayMs = 220) {
    if (this.mode !== "tauri") return;
    if (!(this.state.top.query || "").trim()) return;
    if (this._refreshTimer) clearTimeout(this._refreshTimer);
    this._refreshTimer = setTimeout(() => {
      this._refreshTimer = null;
      this.runSearch();
    }, delayMs);
  }

  async pickRootFolder() {
    if (this.mode !== "tauri") {
      this.state.app.status = "Folder picker requires desktop mode";
      this.paint();
      return;
    }
    try {
      const picked = await invokeDesktop("pick_folder", {});
      if (picked && String(picked).trim()) {
        this.state.app.root = String(picked);
        this.state.app.status = `Root set: ${this.state.app.root}`;
      } else {
        this.state.app.status = "Folder selection cancelled";
      }
    } catch (err) {
      this.state.app.status = `Folder picker error: ${err}`;
    }
    this.paint();
  }

  async runDoctor() {
    if (this.mode !== "tauri") {
      this.state.app.status = "Doctor requires desktop mode";
      this.paint();
      return;
    }
    try {
      const report = await invokeDesktop("doctor", { req: { root: this.state.app.root || "" } });
      this.state.app.status = `Doctor ok | checks: ${(report.checks || []).length}`;
    } catch (err) {
      this.state.app.status = `Doctor error: ${err}`;
    }
    this.paint();
  }

  async runBuild(metadataOnly = false) {
    if (this.mode !== "tauri") {
      this.state.app.status = "Build requires desktop mode";
      this.paint();
      return;
    }
    this.state.top.busy = true;
    this.state.app.status = metadataOnly ? "Syncing monitor..." : "Building index...";
    this.paint();
    try {
      const stats = await invokeDesktop("build_index", {
        req: { root: this.state.app.root || "", metadata_only: metadataOnly },
      });
      this.state.app.status = `Index done | scanned:${stats.scanned} new:${stats.indexed_new} updated:${stats.indexed_updated}`;
    } catch (err) {
      this.state.app.status = `Build error: ${err}`;
    } finally {
      this.state.top.busy = false;
      this.paint();
    }
  }

  toggleMonitor() {
    if (this.monitorTimer) {
      clearInterval(this.monitorTimer);
      this.monitorTimer = null;
      this.state.top.watch_running = false;
      this.state.app.status = "Monitor stopped";
      this.paint();
      return;
    }
    this.state.top.watch_running = true;
    this.state.app.status = "Monitor started";
    this.paint();
    this.monitorTimer = setInterval(async () => {
      if (this.state.top.busy) return;
      await this.runBuild(true);
    }, 4000);
  }

  pushChatMessage(role, text) {
    const rp = this.state.right_panel;
    const safeRole = role === "user" || role === "assistant" || role === "system" ? role : "assistant";
    const safeText = String(text || "").trim();
    if (!safeText) return;
    if (!Array.isArray(rp.chat_messages)) rp.chat_messages = [];
    rp.chat_messages.push({ role: safeRole, text: safeText });
    if (rp.chat_messages.length > 20) {
      rp.chat_messages = rp.chat_messages.slice(-20);
    }
  }

  async sendChatMessage(rawQuestion) {
    const rp = this.state.right_panel;
    const question = String(rawQuestion || "").trim();
    if (!question) return;
    if (!rp.selected_path) {
      this.state.app.status = "Select a file first";
      this.paint();
      return;
    }
    if (rp.chat_busy) return;

    this.pushChatMessage("user", question);
    rp.chat_input = "";
    rp.chat_busy = true;
    this.state.app.status = "Asking document...";
    this.paint();

    if (this.mode !== "tauri") {
      this.pushChatMessage("assistant", "Desktop mode required for document chat.");
      rp.chat_busy = false;
      this.state.app.status = "Chat requires desktop mode";
      this.paint();
      return;
    }

    try {
      const res = await invokeDesktop("ask_document", {
        req: {
          root: this.state.app.root || "",
          document_path: rp.selected_path,
          question,
          mode: rp.chat_mode || "extractive",
        },
      });
      const answer = String((res && res.answer) || "").trim() || "No answer.";
      this.pushChatMessage("assistant", answer);
      this.state.app.status = "Chat answer ready";
    } catch (err) {
      const msg = `Chat error: ${err}`;
      this.pushChatMessage("assistant", msg);
      this.state.app.status = msg;
    } finally {
      rp.chat_busy = false;
      this.paint();
    }
  }
}

class LupaLeft extends HTMLElement {
  bind(shell) {
    const s = shell.state;
    const iconMap = {
      recents: "clock",
      documents: "file",
      images: "image",
      media: "media",
      source: "code",
      pdf: "pdf",
    };
    const collections = (s.sidebar.collections || [])
      .map((c) => {
        const active = c.key === s.sidebar.selected_filter ? " active" : "";
        const icon = iconMap[c.key] || "file";
        return `<button class="collection-btn${active}" data-key="${esc(c.key)}">
          <span class="collection-left"><span class="collection-icon">${iconImg(icon)}</span><span>${esc(c.label)}</span></span>
          <span class="count-pill">${c.count}</span>
        </button>`;
      })
      .join("");

    this.innerHTML = `
      <aside class="left">
        <div class="left-scroll">
          <p class="tiny-title">SYSTEM TOOLS</p>
          <button class="tool-btn" id="btn-build"><span class="tool-glyph">${iconImg("hammer")}</span>Build Index</button>
          <button class="tool-btn" id="btn-monitor"><span class="tool-glyph">${iconImg("pulse")}</span>${s.top.watch_running ? "Stop Monitor" : "Start Monitor"}</button>
          <button class="tool-btn" id="btn-doctor"><span class="tool-glyph">${iconImg("shield")}</span>System Doctor</button>
          <div class="rule"></div>
          <p class="tiny-title">COLLECTIONS</p>
          ${collections}
          <div class="rule"></div>
          <p class="tiny-title">INDEX PATH</p>
          <div class="path-row">
            <div class="path-chip">
              <span class="mono path-value">${esc(s.app.root || "-")}</span>
            </div>
            <button class="path-next" id="btn-root" title="Select folder">${iconImg("folder-open")}</button>
          </div>
          <div class="rule"></div>
          <p class="tiny-title">ADVANCED SEARCH</p>
          <input class="adv-chip mono" id="adv-regex" placeholder="Regex (e.g. pdf|rs)" value="${esc(s.sidebar.regex || "")}" />
          <input class="adv-chip mono" id="adv-path" placeholder="Path prefix (e.g. project)" value="${esc(s.sidebar.path_prefix || "")}" />
          <input class="adv-chip mono" id="adv-limit" type="number" min="1" max="200" value="${Number(s.sidebar.limit || 20)}" />
          <div class="toggle-row">
            <span class="toggle-pill ${s.sidebar.show_snippets !== false ? "on" : "off"}" id="adv-snippets"></span>
            <span>Show text snippets</span>
          </div>
        </div>
        <div class="left-foot">LUPA v2.1.0 | Index ${esc(s.app.root || "-")}</div>
      </aside>
    `;

    this.querySelectorAll(".collection-btn").forEach((el) => {
      el.addEventListener("click", () => {
        shell.state.sidebar.selected_filter = el.getAttribute("data-key") || "recents";
        shell.state.results.visible_count = Math.min(
          (shell.state.results.items || []).length,
          effectiveLimit(shell.state),
        );
        shell.state.sidebar.collections = withCollectionCounts(
          shell.state.results.items || [],
          shell.state.sidebar.selected_filter,
        );
        shell.selectFirstFromActiveCollection();
      });
    });

    const doctorBtn = this.querySelector("#btn-doctor");
    const monitorBtn = this.querySelector("#btn-monitor");
    const rootBtn = this.querySelector("#btn-root");
    const regexInput = this.querySelector("#adv-regex");
    const pathInput = this.querySelector("#adv-path");
    const limitInput = this.querySelector("#adv-limit");
    const snippetsToggle = this.querySelector("#adv-snippets");
    if (doctorBtn) doctorBtn.addEventListener("click", async () => shell.runDoctor());
    if (monitorBtn) monitorBtn.addEventListener("click", () => shell.toggleMonitor());
    if (rootBtn) rootBtn.addEventListener("click", async () => shell.pickRootFolder());
    if (regexInput) {
      regexInput.addEventListener("input", (ev) => {
        shell.state.sidebar.regex = ev.target.value || "";
      });
    }
    if (pathInput) {
      pathInput.addEventListener("input", (ev) => {
        shell.state.sidebar.path_prefix = ev.target.value || "";
      });
    }
    if (limitInput) {
      limitInput.addEventListener("input", (ev) => {
        const n = Number(ev.target.value);
        shell.state.sidebar.limit = Number.isFinite(n) && n > 0 ? Math.min(n, 200) : 20;
      });
      limitInput.addEventListener("change", () => {
        shell.state.results.visible_count = Math.min(
          (shell.state.results.items || []).length,
          effectiveLimit(shell.state),
        );
        shell.paint();
        shell.scheduleSearchRefresh(80);
      });
      limitInput.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") {
          ev.preventDefault();
          shell.state.results.visible_count = Math.min(
            (shell.state.results.items || []).length,
            effectiveLimit(shell.state),
          );
          shell.paint();
          shell.scheduleSearchRefresh(0);
        }
      });
    }
    if (snippetsToggle) {
      snippetsToggle.addEventListener("click", () => {
        shell.state.sidebar.show_snippets = !(shell.state.sidebar.show_snippets !== false);
        shell.paint();
      });
    }

    const buildBtn = this.querySelector("#btn-build");
    if (buildBtn) {
      buildBtn.addEventListener("click", async () => shell.runBuild(false));
    }
  }
}

class LupaCenter extends HTMLElement {
  bind(shell) {
    const s = shell.state;
    const query = esc(s.top.query || "");
    const ms = s.results.took_ms == null ? "N/A" : `${s.results.took_ms}ms`;
    const filtered = [];
    (s.results.items || []).forEach((it, idx) => {
      if (itemMatchesCollection(it, s.sidebar.selected_filter || "recents")) {
        filtered.push({ ...it, __idx: idx });
      }
    });
    const visibleCount = Math.max(
      1,
      Math.min(filtered.length, Number(s.results.visible_count || effectiveLimit(s))),
    );
    const items = filtered.slice(0, visibleCount);
    const remaining = Math.max(0, filtered.length - items.length);
    const rows = items
      .map((r, rowPos) => {
        const active = r.selected ? " active" : "";
        const badge = esc((r.ext || "file").toUpperCase());
        const snip = r.snippet
          ? `<div class="row-snippet">${markSnippet(r.snippet, s.top.query)}</div>`
          : "";
        const thumb = isImageExt(r.ext) && rowPos < 80
          ? `<img class="row-thumb" src="${esc(fileSrc(r.path))}" alt="${esc(r.name)}" loading="lazy" decoding="async" />`
          : `<div class="file-icon ${extClass(r.ext)}">${extLabel(r.ext)}</div>`;
        return `<article class="row${active}" data-row="${r.__idx}">
          ${thumb}
          <div class="row-main">
            <div class="row-name">
              <span>${esc(r.name)}</span>
              <span class="type-badge">${badge}</span>
            </div>
            <div class="row-path mono">${esc(r.path)}</div>
            ${snip}
            <div class="meta">- ${esc(r.size || "-")} - Modified ${esc(r.modified || "-")}</div>
          </div>
          <div class="rank">#${r.rank}</div>
        </article>`;
      })
      .join("");

    this.innerHTML = `
      <main class="center">
        <div class="results-head">
          <div class="head-title-wrap">
            <h2 class="results-title">Search Results</h2>
            <div class="results-sub">for <b>"${query}"</b></div>
          </div>
          <span class="chip chip-purple">${s.results.total_hits} hits</span>
          <span class="chip chip-green">${ms}</span>
        </div>
        <section class="rows">
          ${
            rows ||
            '<article class="row row-empty"><div class="empty-block"><h3>No results yet</h3><p>Run a search from the top bar to see indexed files.</p></div></article>'
          }
        </section>
        ${
          remaining > 0
            ? `<button class="load-more" id="load-more-btn">Load more results | ${remaining} remaining</button>`
            : ""
        }
      </main>
    `;

    this.querySelectorAll(".row[data-row]").forEach((el) => {
      el.addEventListener("click", () => {
        const idx = Number(el.getAttribute("data-row"));
        shell.selectItem(idx);
      });
      el.addEventListener("dblclick", async () => {
        const idx = Number(el.getAttribute("data-row"));
        if (!Number.isFinite(idx)) return;
        const item = (shell.state.results.items || [])[idx];
        if (!item) return;
        await shell.runPathAction("open", item.path);
      });
    });

    const loadMore = this.querySelector("#load-more-btn");
    if (loadMore) {
      loadMore.addEventListener("click", () => {
        const current = Number(shell.state.results.visible_count || effectiveLimit(shell.state));
        const step = Math.max(10, effectiveLimit(shell.state));
        shell.state.results.visible_count = Math.min(
          (shell.state.results.items || []).length,
          current + step,
        );
        shell.paint();
      });
    }
  }
}

class LupaRight extends HTMLElement {
  bind(shell) {
    const s = shell.state;
    const rp = s.right_panel || {};
    const preview = rp.tab !== "chat";
    const mode = rp.chat_mode === "local_model" ? "local" : "extractive";
    const path = esc(rp.selected_path || "-");
    const file = esc(rp.file_name || "No file selected");
    const fileType = esc(rp.file_type || "-");
    const size = esc(rp.size || "-");
    const created = esc(rp.created || "-");
    const modified = esc(rp.modified || "-");
    const snippet = rp.snippet ? markSnippet(rp.snippet, s.top.query) : "Sin fragmento para este formato o contenido.";
    const matchCount = Number(rp.match_count || 0);
    const previewMedia = isImageExt(rp.file_type)
      ? `<img class="preview-image" src="${esc(fileSrc(rp.selected_path))}" alt="${file}" loading="lazy" decoding="async" />`
      : `<div class="preview-icon">${extLabel(rp.file_type)}</div><div class="preview-text">No preview available</div>`;

    const messages = (rp.chat_messages || [])
      .slice(-12)
      .map((m) => {
        if (m && typeof m === "object") {
          const cls = m.role === "user" ? "user" : m.role === "system" ? "system" : "assistant";
          return `<div class="chat-bubble ${cls}">${esc(m.text || "")}</div>`;
        }
        return `<div class="chat-bubble assistant">${esc(String(m || ""))}</div>`;
      })
      .join("");

    this.innerHTML = `
      <aside class="right">
        <div class="right-head">
          <div class="tab-wrap">
            <button class="tab ${preview ? "active" : ""}" id="tab-preview">Preview</button>
            <button class="tab ${!preview ? "active" : ""}" id="tab-chat">AI Chat</button>
          </div>
          <button class="close" id="panel-close">x</button>
        </div>
        ${
          preview
            ? `
          <div class="right-scroll">
            <section class="right-block">
              <p class="tiny-title">VISTA PREVIA</p>
              <div class="preview-card">
                ${previewMedia}
              </div>
            </section>
            <section class="right-block">
              <p class="tiny-title">FILE INFO</p>
              <div class="info-grid">
                <div class="info-row"><span class="k">NAME</span><span class="v">${file}</span></div>
                <div class="info-row"><span class="k">PATH</span><span class="v mono">${path}</span></div>
                <div class="info-row"><span class="k">TYPE</span><span class="v"><span class="mini-badge">${fileType} | ${size}</span></span></div>
                <div class="info-row"><span class="k">CREATED</span><span class="v">${created}</span></div>
                <div class="info-row"><span class="k">MODIFIED</span><span class="v">${modified}</span></div>
              </div>
            </section>
            <section class="right-block">
              <p class="tiny-title">ACTIONS</p>
              <div class="action-grid">
                <button class="action-btn" data-action="open">Open</button>
                <button class="action-btn" data-action="open_at_match">Open at match</button>
                <button class="action-btn" data-action="open_with">Open with...</button>
                <button class="action-btn" data-action="folder">Folder</button>
                <button class="action-btn" data-action="copy_path">Copy path</button>
                <button class="action-btn action-primary" id="ask-doc">Ask this doc</button>
              </div>
            </section>
            <section class="right-block">
              <p class="tiny-title">COINCIDENCIA EN DOCUMENTO</p>
              <div class="snippet-box">
                <div class="snippet-meta">Matches found: ${matchCount}</div>
                <div class="snippet-text">${snippet}</div>
              </div>
            </section>
            <section class="right-block">
              <p class="tiny-title">METRICAS</p>
              <div class="metrics-list">
                <div class="metric-row"><span>Resultados</span><span>${s.results.total_hits}</span></div>
                <div class="metric-row"><span>Tiempo busqueda</span><span>${s.results.took_ms == null ? "N/A" : `${s.results.took_ms}ms`}</span></div>
                <div class="metric-row"><span>Indexados</span><span>${s.top.hits || 0}</span></div>
                <div class="metric-row"><span>Watch</span><span>${s.top.watch_running ? "ON" : "OFF"}</span></div>
              </div>
            </section>
          </div>
        `
            : `
          <div class="right-scroll chat-layout">
            <div class="chat-mode">
              <button class="${mode === "extractive" ? "active" : ""}" id="mode-ext">Extractive</button>
              <button class="${mode === "local" ? "active" : ""}" id="mode-local">Local AI</button>
            </div>
            <div class="chat-doc">
              <div class="file-icon ${extClass(rp.file_type)}">${extLabel(rp.file_type)}</div>
              <div>
                <div class="chat-doc-name">${file}</div>
                <div class="chat-doc-path mono">${path}</div>
              </div>
            </div>
            <div class="chat-quick">
              <button class="quick active" data-quick="summary">Summary</button>
              <button class="quick" data-quick="key_dates">Key dates</button>
              <button class="quick" data-quick="main_topic">Main topic</button>
            </div>
            <div class="chat-feed">
              <div class="chat-bubble system">Ready. Ask about '${file}'.</div>
              ${messages || '<div class="chat-bubble assistant">I can answer from extracted snippets and file metadata in local mode.</div>'}
            </div>
            <div class="chat-composer">
              <textarea class="chat-input" placeholder="Ask about this document...">${esc(rp.chat_input || "")}</textarea>
              <div class="chat-actions">
                <button class="send-btn"${rp.chat_busy ? " disabled" : ""}>${rp.chat_busy ? "Sending..." : "Send"}</button>
                <button class="reset-btn"${rp.chat_busy ? " disabled" : ""}>O</button>
              </div>
            </div>
          </div>
        `
        }
      </aside>
    `;

    const previewTab = this.querySelector("#tab-preview");
    const chatTab = this.querySelector("#tab-chat");
    const closeBtn = this.querySelector("#panel-close");
    const askDoc = this.querySelector("#ask-doc");
    const extMode = this.querySelector("#mode-ext");
    const localMode = this.querySelector("#mode-local");
    const quickButtons = this.querySelectorAll(".quick[data-quick]");
    const chatFeed = this.querySelector(".chat-feed");
    const chatInput = this.querySelector(".chat-input");
    const sendBtn = this.querySelector(".send-btn");
    const resetBtn = this.querySelector(".reset-btn");
    const actionButtons = this.querySelectorAll(".action-btn[data-action]");

    if (previewTab) {
      previewTab.addEventListener("click", () => {
        shell.state.right_panel.tab = "preview";
        shell.paint();
      });
    }
    if (chatTab) {
      chatTab.addEventListener("click", () => {
        shell.state.right_panel.tab = "chat";
        shell.paint();
      });
    }
    if (closeBtn) {
      closeBtn.addEventListener("click", () => {
        shell.state.right_panel.visible = false;
        shell.paint();
      });
    }
    if (askDoc) {
      askDoc.addEventListener("click", () => {
        shell.state.right_panel.tab = "chat";
        shell.paint();
      });
    }
    actionButtons.forEach((btn) => {
      btn.addEventListener("click", async () => {
        const action = btn.getAttribute("data-action");
        await shell.runPathAction(action, rp.selected_path);
      });
    });
    if (extMode) {
      extMode.addEventListener("click", () => {
        shell.state.right_panel.chat_mode = "extractive";
        shell.paint();
      });
    }
    if (localMode) {
      localMode.addEventListener("click", () => {
        shell.state.right_panel.chat_mode = "local_model";
        shell.paint();
      });
    }
    quickButtons.forEach((btn) => {
      btn.addEventListener("click", async () => {
        const key = btn.getAttribute("data-quick");
        let prompt = "Summarize this document.";
        if (key === "key_dates") prompt = "List key dates mentioned in this document.";
        if (key === "main_topic") prompt = "What is the main topic of this document?";
        await shell.sendChatMessage(prompt);
      });
    });
    if (chatInput) {
      chatInput.addEventListener("input", (ev) => {
        shell.state.right_panel.chat_input = ev.target.value || "";
      });
      chatInput.addEventListener("keydown", async (ev) => {
        if (ev.key === "Enter" && !ev.shiftKey) {
          ev.preventDefault();
          await shell.sendChatMessage(shell.state.right_panel.chat_input || "");
        }
      });
    }
    if (sendBtn) {
      sendBtn.addEventListener("click", async () => {
        await shell.sendChatMessage(shell.state.right_panel.chat_input || "");
      });
    }
    if (resetBtn) {
      resetBtn.addEventListener("click", () => {
        shell.state.right_panel.chat_messages = [];
        shell.state.right_panel.chat_input = "";
        shell.paint();
      });
    }
    if (chatFeed) {
      requestAnimationFrame(() => {
        chatFeed.scrollTop = chatFeed.scrollHeight;
      });
    }
  }
}

customElements.define("lupa-left", LupaLeft);
customElements.define("lupa-center", LupaCenter);
customElements.define("lupa-right", LupaRight);
customElements.define("lupa-shell", LupaShell);
