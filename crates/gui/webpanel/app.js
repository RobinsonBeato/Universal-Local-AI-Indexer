import { LANG_STORAGE_KEY, collectionLabel, resolveLang, t as translate } from "./components/i18n.index.js";

const EMPTY = {
  generated_at: "",
  app: { status: "Ready", root: "" },
  top: { query: "", busy: false, watch_running: false, hits: 0, latency_ms: null, cpu_pct: null },
  settings: {
    language: "es",
    open: false,
    onboarding_done: false,
    terms_accepted: false,
    ai_opt_in: false,
    ai_installing: false,
  },
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

const SETTINGS_STORAGE_KEY = "lupa.settings.v1";

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

function withCollectionCounts(items, selectedFilter, lang = "es") {
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
    { key: "recents", label: collectionLabel(lang, "recents"), count: counts.recents },
    { key: "documents", label: collectionLabel(lang, "documents"), count: counts.documents },
    { key: "images", label: collectionLabel(lang, "images"), count: counts.images },
    { key: "media", label: collectionLabel(lang, "media"), count: counts.media },
    { key: "source", label: collectionLabel(lang, "source"), count: counts.source },
    { key: "pdf", label: collectionLabel(lang, "pdf"), count: counts.pdf },
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
      snippet_loaded: !!h.snippet,
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
  lang() {
    return resolveLang(this.state?.settings?.language);
  }

  t(key, vars = {}) {
    return translate(this.lang(), key, vars);
  }

  loadSettings() {
    const defaults = {
      language: "es",
      open: false,
      onboarding_done: false,
      terms_accepted: false,
      ai_opt_in: false,
      ai_installing: false,
    };
    try {
      const raw = localStorage.getItem(SETTINGS_STORAGE_KEY);
      const parsed = raw ? JSON.parse(raw) : {};
      this.state.settings = { ...defaults, ...parsed, open: false, ai_installing: false };
      const legacyLang = localStorage.getItem(LANG_STORAGE_KEY);
      if (legacyLang) {
        this.state.settings.language = resolveLang(legacyLang);
      }
    } catch {
      this.state.settings = { ...defaults };
    }
    this.state.settings.language = resolveLang(this.state.settings.language);
    this.state.sidebar.collections = withCollectionCounts(
      this.state.results.items || [],
      this.state.sidebar.selected_filter || "recents",
      this.lang(),
    );
  }

  persistSettings() {
    try {
      localStorage.setItem(
        SETTINGS_STORAGE_KEY,
        JSON.stringify({
          language: resolveLang(this.state.settings.language),
          onboarding_done: !!this.state.settings.onboarding_done,
          terms_accepted: !!this.state.settings.terms_accepted,
          ai_opt_in: !!this.state.settings.ai_opt_in,
        }),
      );
      localStorage.setItem(LANG_STORAGE_KEY, resolveLang(this.state.settings.language));
    } catch {
      // ignore storage errors
    }
  }

  setLanguage(lang) {
    const next = resolveLang(lang);
    this.state.settings.language = next;
    this.state.settings.open = false;
    this.state.sidebar.collections = withCollectionCounts(
      this.state.results.items || [],
      this.state.sidebar.selected_filter || "recents",
      next,
    );
    this.persistSettings();
    this.paint();
  }

  completeOnboarding() {
    if (!this.state.settings.terms_accepted) {
      this.state.app.status = this.t("onboarding_required_terms");
      this.paint();
      return;
    }
    this.state.settings.onboarding_done = true;
    this.state.right_panel.chat_mode = this.state.settings.ai_opt_in ? "local_model" : "extractive";
    this.persistSettings();
    this.state.app.status = this.t("app_ready");
    this.paint();
  }

  async installLocalAiRuntime() {
    if (this.mode !== "tauri") return;
    if (this.state.settings.ai_installing) return;
    this.state.settings.ai_installing = true;
    this.state.app.status = this.t("onboarding_ai_installing");
    this.paint();
    try {
      await invokeDesktop("install_local_ai", {});
      this.state.settings.ai_opt_in = true;
      this.persistSettings();
      this.state.app.status = this.t("onboarding_ai_ready");
    } catch (err) {
      this.state.app.status = this.t("onboarding_ai_error", { error: err });
    } finally {
      this.state.settings.ai_installing = false;
      this.paint();
    }
  }

  connectedCallback() {
    this.state = JSON.parse(JSON.stringify(EMPTY));
    this.monitorTimer = null;
    this._hotkeysBound = false;
    this._refreshTimer = null;
    this._searchToken = 0;
    this._progressiveTimer = null;
    this._snippetTimer = null;
    this._snippetBusy = false;
    this._cpuTimer = null;
    this.loadSettings();
    this.mode = tauriInvoke() ? "tauri" : "bridge";
    requestAnimationFrame(() => this.paint());

    if (this.mode === "bridge") {
      this.timer = setInterval(async () => {
        const next = await loadBridgeState();
        next.settings = { ...(this.state.settings || {}), ...(next.settings || {}) };
        if (JSON.stringify(next) !== JSON.stringify(this.state)) {
          this.state = next;
          this.state.settings.language = resolveLang(this.state.settings.language);
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
    if (this._progressiveTimer) clearInterval(this._progressiveTimer);
    if (this._snippetTimer) clearInterval(this._snippetTimer);
    if (this._refreshTimer) clearTimeout(this._refreshTimer);
    if (this._cpuTimer) clearInterval(this._cpuTimer);
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
    this.state.app.status = this.t("app_ready");
    this.paint();
    this.startCpuTelemetry();
  }

  startCpuTelemetry() {
    if (this.mode !== "tauri") return;
    if (this._cpuTimer) return;
    const tick = async () => {
      try {
        const value = await invokeDesktop("cpu_usage", {});
        const n = Number(value);
        if (Number.isFinite(n)) {
          this.state.top.cpu_pct = Math.max(0, Math.min(100, n));
          const cpuNode = this.querySelector(".metrics .w");
          if (cpuNode) {
            cpuNode.textContent = `${Math.round(this.state.top.cpu_pct)}%`;
          }
        }
      } catch {
        // Keep UI stable if telemetry call fails.
      }
    };
    tick();
    this._cpuTimer = setInterval(tick, 1000);
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
    const stateText = s.top.busy ? this.t("status_indexing") : this.t("status_idle");
    const cpu = Number.isFinite(s.top.cpu_pct) ? `${Math.round(s.top.cpu_pct)}%` : "N/A";
    const rightCol = s.right_panel.visible === false ? "0px" : "340px";
    const leftCol = "236px";
    const settingsOpen = s.settings?.open === true;
    const onboardingOpen = s.settings?.onboarding_done !== true;
    const lang = this.lang();

    this.innerHTML = `
      <div class="app-shell">
        <header class="topbar">
          <div class="brand">
            <div class="logo"></div>
            <div class="brand-text">LUPA</div>
          </div>
          <div class="v-divider"></div>
          <div class="search-wrap">
            <span class="search-glyph">${iconImg("search")}</span>
            <input class="search-input" value="${esc(s.top.query || "")}" placeholder="${esc(this.t("search_placeholder"))}" />
            <div class="kbd-group">
              <span class="kbd-chip">Ctrl</span>
              <span class="kbd-chip">K</span>
            </div>
          </div>
          <button class="search-btn" id="btn-search">${esc(this.t("search_button"))}</button>
          <div class="state-pill"><span class="state-dot"></span>${stateText}</div>
          <div class="topbar-right">
            <div class="settings-wrap">
              <button class="settings-btn" id="btn-settings" title="${esc(this.t("settings_label"))}" aria-label="${esc(this.t("settings_label"))}">
                ${iconImg("settings")}
              </button>
              ${
                settingsOpen
                  ? `<div class="settings-menu">
                      <div class="settings-title">${esc(this.t("settings_language"))}</div>
                      <button class="lang-item ${lang === "es" ? "active" : ""}" data-lang="es">${esc(this.t("language_es"))}</button>
                      <button class="lang-item ${lang === "en" ? "active" : ""}" data-lang="en">${esc(this.t("language_en"))}</button>
                      <div class="settings-divider"></div>
                      <button class="settings-link" id="settings-open-license">${esc(this.t("settings_terms"))}</button>
                      <label class="settings-toggle-row">
                        <input type="checkbox" id="settings-ai-optin" ${s.settings.ai_opt_in ? "checked" : ""} />
                        <span>${esc(this.t("settings_ai"))}</span>
                      </label>
                    </div>`
                  : ""
              }
            </div>
            <div class="metrics">
              <span>${esc(this.t("metrics_cpu"))} <span class="w">${cpu}</span></span>
              <span>${esc(this.t("metrics_lat"))} <span class="metric">${lat}</span></span>
            </div>
          </div>
        </header>
        <section class="main-grid" style="grid-template-columns: ${leftCol} 1fr ${rightCol};">
          <lupa-left></lupa-left>
          <lupa-center></lupa-center>
          ${s.right_panel.visible === false ? "<div></div>" : "<lupa-right></lupa-right>"}
        </section>
        <footer class="statusbar">
          <span>${esc(s.app.status || this.t("app_ready"))}</span>
          <span>${esc(this.t("statusbar_results", { hits: s.results.total_hits, lat, root: s.app.root || "-" }))}</span>
        </footer>
        ${
          onboardingOpen
            ? `<div class="onboarding-backdrop">
                 <section class="onboarding-modal">
                   <h2>${esc(this.t("onboarding_title"))}</h2>
                   <p>${esc(this.t("onboarding_subtitle"))}</p>
                   <div class="onboarding-row">
                     <button class="lang-item ${lang === "es" ? "active" : ""}" data-lang="es">${esc(this.t("language_es"))}</button>
                     <button class="lang-item ${lang === "en" ? "active" : ""}" data-lang="en">${esc(this.t("language_en"))}</button>
                   </div>
                   <label class="onboarding-check">
                     <input type="checkbox" id="onboard-terms" ${s.settings.terms_accepted ? "checked" : ""} />
                     <span>${esc(this.t("onboarding_terms_label"))}</span>
                   </label>
                   <button class="settings-link" id="onboard-open-license">${esc(this.t("onboarding_terms_open"))}</button>
                   <label class="onboarding-check">
                     <input type="checkbox" id="onboard-ai" ${s.settings.ai_opt_in ? "checked" : ""} />
                     <span>${esc(this.t("onboarding_ai_label"))}</span>
                   </label>
                   <p class="onboarding-hint">${esc(this.t("onboarding_ai_hint"))}</p>
                   <div class="onboarding-actions">
                     <button class="action-btn" id="onboard-install-ai" ${this.mode !== "tauri" || s.settings.ai_installing ? "disabled" : ""}>${
                       esc(s.settings.ai_installing ? this.t("onboarding_ai_installing") : this.t("onboarding_ai_install"))
                     }</button>
                     <button class="search-btn" id="onboard-continue" ${s.settings.terms_accepted ? "" : "disabled"}>${esc(this.t("onboarding_continue"))}</button>
                   </div>
                 </section>
               </div>`
            : ""
        }
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
    const settingsBtn = this.querySelector("#btn-settings");
    if (input) {
      input.addEventListener("input", (ev) => {
        this.state.top.query = ev.target.value;
      });
      input.addEventListener("keydown", async (ev) => {
        if (ev.key === "Enter") await this.runSearch();
      });
    }
    if (btn) btn.addEventListener("click", async () => this.runSearch());
    if (settingsBtn) {
      settingsBtn.addEventListener("click", (ev) => {
        ev.stopPropagation();
        this.state.settings.open = !this.state.settings.open;
        this.paint();
      });
    }
    this.querySelectorAll(".lang-item[data-lang]").forEach((el) => {
      el.addEventListener("click", (ev) => {
        ev.stopPropagation();
        this.setLanguage(el.getAttribute("data-lang") || "es");
      });
    });
    const settingsOpenLicense = this.querySelector("#settings-open-license");
    if (settingsOpenLicense) {
      settingsOpenLicense.addEventListener("click", async () => {
        if (this.mode === "tauri") {
          await this.runPathAction("open", "LICENSE");
        } else {
          window.open("./LICENSE", "_blank", "noopener,noreferrer");
        }
      });
    }
    const settingsAiOptin = this.querySelector("#settings-ai-optin");
    if (settingsAiOptin) {
      settingsAiOptin.addEventListener("change", (ev) => {
        this.state.settings.ai_opt_in = !!ev.target.checked;
        this.persistSettings();
      });
    }
    if (settingsOpen) {
      document.addEventListener(
        "click",
        () => {
          if (this.state.settings.open) {
            this.state.settings.open = false;
            this.paint();
          }
        },
        { once: true },
      );
    }

    const onboardTerms = this.querySelector("#onboard-terms");
    if (onboardTerms) {
      onboardTerms.addEventListener("change", (ev) => {
        this.state.settings.terms_accepted = !!ev.target.checked;
        this.persistSettings();
        this.paint();
      });
    }
    const onboardAi = this.querySelector("#onboard-ai");
    if (onboardAi) {
      onboardAi.addEventListener("change", (ev) => {
        this.state.settings.ai_opt_in = !!ev.target.checked;
        this.persistSettings();
      });
    }
    const onboardContinue = this.querySelector("#onboard-continue");
    if (onboardContinue) {
      onboardContinue.addEventListener("click", () => this.completeOnboarding());
    }
    const onboardInstallAi = this.querySelector("#onboard-install-ai");
    if (onboardInstallAi) {
      onboardInstallAi.addEventListener("click", async () => this.installLocalAiRuntime());
    }
    const onboardOpenLicense = this.querySelector("#onboard-open-license");
    if (onboardOpenLicense) {
      onboardOpenLicense.addEventListener("click", async () => {
        if (this.mode === "tauri") {
          await this.runPathAction("open", "LICENSE");
        } else {
          window.open("./LICENSE", "_blank", "noopener,noreferrer");
        }
      });
    }

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
      this.state.app.status = this.t("no_file_selected");
      this.paint();
      return;
    }
    if (this.mode !== "tauri") {
      this.state.app.status = this.t("action_requires_desktop");
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
      this.state.app.status = this.t("action_done", { action });
    } catch (err) {
      this.state.app.status = this.t("action_error", { error: err });
    }
    this.paint();
  }

  async runSearch() {
    const query = (this.state.top.query || "").trim();
    if (!query) {
      this.state.app.status = this.t("type_a_query");
      this.paint();
      return;
    }
    const token = ++this._searchToken;
    if (this._progressiveTimer) {
      clearInterval(this._progressiveTimer);
      this._progressiveTimer = null;
    }
    if (this._snippetTimer) {
      clearInterval(this._snippetTimer);
      this._snippetTimer = null;
    }
    this._snippetBusy = false;
    this.state.results.items = [];
    this.state.results.total_hits = 0;
    this.state.results.took_ms = null;
    this.state.results.visible_count = 0;
    this.state.top.hits = 0;
    this.state.sidebar.collections = withCollectionCounts(
      [],
      this.state.sidebar.selected_filter || "recents",
      this.lang(),
    );
    this.state.top.busy = true;
    this.state.app.status = this.t("searching", { query });
    this.paint();

    if (this.mode !== "tauri") {
      this.state.top.busy = false;
      this.state.app.status = this.t("bridge_search_only");
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
          highlight: false,
        },
      });

      const mapped = mapSearchResult(query, res);
      if (token !== this._searchToken) {
        return;
      }
      this.state.results.total_hits = mapped.total_hits;
      this.state.results.took_ms = mapped.took_ms;
      this.state.results.items = mapped.items;
      const limit = effectiveLimit(this.state);
      const target = Math.min(mapped.items.length, limit);
      const firstBatch = Math.min(target, 40);
      this.state.results.visible_count = firstBatch;
      this.state.top.latency_ms = mapped.took_ms;
      this.state.top.hits = mapped.total_hits;
      this.state.top.query = query;
      this.state.sidebar.collections = withCollectionCounts(
        mapped.items,
        this.state.sidebar.selected_filter || "recents",
        this.lang(),
      );
      this.state.app.status =
        target > firstBatch
          ? this.t("results_rendering", { hits: mapped.total_hits, done: firstBatch, total: target })
          : this.t("results_done", { hits: mapped.total_hits });
      this.selectFirstFromActiveCollection();
      this.scheduleProgressiveReveal(token, target);
      this.scheduleSnippetHydration(token, query);
    } catch (err) {
      if (token !== this._searchToken) {
        return;
      }
      this.state.app.status = this.t("search_error", { error: err });
      this.state.top.busy = false;
      this.paint();
      return;
    }
    if (token === this._searchToken) {
      this.state.top.busy = false;
      this.paint();
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

  scheduleProgressiveReveal(token, targetVisible) {
    if (targetVisible <= (this.state.results.visible_count || 0)) return;
    if (this._progressiveTimer) {
      clearInterval(this._progressiveTimer);
      this._progressiveTimer = null;
    }
    const step = targetVisible > 140 ? 30 : 20;
    this._progressiveTimer = setInterval(() => {
      if (token !== this._searchToken) {
        clearInterval(this._progressiveTimer);
        this._progressiveTimer = null;
        return;
      }
      const current = Number(this.state.results.visible_count || 0);
      const next = Math.min(targetVisible, current + step);
      this.state.results.visible_count = next;
      if (next >= targetVisible) {
        this.state.app.status = this.t("results_done", { hits: this.state.results.total_hits });
        clearInterval(this._progressiveTimer);
        this._progressiveTimer = null;
      } else {
        this.state.app.status = this.t("results_rendering", {
          hits: this.state.results.total_hits,
          done: next,
          total: targetVisible,
        });
      }
      this.paint();
      this.scheduleSnippetHydration(token, this.state.top.query || "");
    }, 33);
  }

  scheduleSnippetHydration(token, query) {
    if (this.mode !== "tauri") return;
    if (this.state.sidebar.show_snippets === false) return;
    if (!query || !query.trim()) return;
    if (this._snippetTimer) return;

    this._snippetTimer = setInterval(async () => {
      if (token !== this._searchToken) {
        clearInterval(this._snippetTimer);
        this._snippetTimer = null;
        this._snippetBusy = false;
        return;
      }
      if (this._snippetBusy) return;

      const maxVisible = Number(this.state.results.visible_count || 0);
      const pending = (this.state.results.items || [])
        .slice(0, maxVisible)
        .filter((it) => !it.snippet_loaded)
        .slice(0, 12);

      if (pending.length === 0) {
        clearInterval(this._snippetTimer);
        this._snippetTimer = null;
        return;
      }

      this._snippetBusy = true;
      try {
        const res = await invokeDesktop("fetch_snippets", {
          req: {
            root: this.state.app.root || "",
            query,
            paths: pending.map((p) => p.path),
          },
        });
        if (token !== this._searchToken) return;
        const snippetByPath = new Map((res.items || []).map((it) => [String(it.path), String(it.snippet || "")]));
        let changed = false;
        for (const it of this.state.results.items || []) {
          if (!it.snippet_loaded && snippetByPath.has(it.path)) {
            it.snippet = snippetByPath.get(it.path) || "";
            it.snippet_loaded = true;
            changed = true;
          }
        }
        if (changed) this.paint();
      } catch {
        // Keep UI responsive even if snippet batch fails.
      } finally {
        this._snippetBusy = false;
      }
    }, 70);
  }

  async pickRootFolder() {
    if (this.mode !== "tauri") {
      this.state.app.status = this.t("folder_picker_requires_desktop");
      this.paint();
      return;
    }
    try {
      const picked = await invokeDesktop("pick_folder", {});
      if (picked && String(picked).trim()) {
        this.state.app.root = String(picked);
        this.state.app.status = this.t("root_set", { root: this.state.app.root });
      } else {
        this.state.app.status = this.t("folder_selection_cancelled");
      }
    } catch (err) {
      this.state.app.status = this.t("folder_picker_error", { error: err });
    }
    this.paint();
  }

  async runDoctor() {
    if (this.mode !== "tauri") {
      this.state.app.status = this.t("doctor_requires_desktop");
      this.paint();
      return;
    }
    try {
      const report = await invokeDesktop("doctor", { req: { root: this.state.app.root || "" } });
      this.state.app.status = this.t("doctor_ok", { count: (report.checks || []).length });
    } catch (err) {
      this.state.app.status = this.t("doctor_error", { error: err });
    }
    this.paint();
  }

  async runBuild(metadataOnly = false) {
    if (this.mode !== "tauri") {
      this.state.app.status = this.t("build_requires_desktop");
      this.paint();
      return;
    }
    this.state.top.busy = true;
    this.state.app.status = metadataOnly ? this.t("syncing_monitor") : this.t("building_index");
    this.paint();
    try {
      const stats = await invokeDesktop("build_index", {
        req: { root: this.state.app.root || "", metadata_only: metadataOnly },
      });
      this.state.app.status = this.t("index_done", {
        scanned: stats.scanned,
        new: stats.indexed_new,
        updated: stats.indexed_updated,
      });
    } catch (err) {
      this.state.app.status = this.t("build_error", { error: err });
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
      this.state.app.status = this.t("monitor_stopped");
      this.paint();
      return;
    }
    this.state.top.watch_running = true;
    this.state.app.status = this.t("monitor_started");
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
      this.state.app.status = this.t("select_file_first");
      this.paint();
      return;
    }
    if (rp.chat_busy) return;

    this.pushChatMessage("user", question);
    rp.chat_input = "";
    rp.chat_busy = true;
    this.state.app.status = this.t("asking_document");
    this.paint();

    if (this.mode !== "tauri") {
      this.pushChatMessage("assistant", this.t("desktop_mode_required_chat"));
      rp.chat_busy = false;
      this.state.app.status = this.t("chat_requires_desktop");
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
      const answer = String((res && res.answer) || "").trim() || this.t("no_answer");
      this.pushChatMessage("assistant", answer);
      this.state.app.status = this.t("chat_answer_ready");
    } catch (err) {
      const msg = this.t("chat_error", { error: err });
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
    const t = (key, vars = {}) => shell.t(key, vars);
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
          <p class="tiny-title">${esc(t("system_tools"))}</p>
          <button class="tool-btn" id="btn-build"><span class="tool-glyph">${iconImg("hammer")}</span>${esc(t("build_index"))}</button>
          <button class="tool-btn" id="btn-monitor"><span class="tool-glyph">${iconImg("pulse")}</span>${esc(
            s.top.watch_running ? t("stop_monitor") : t("start_monitor"),
          )}</button>
          <button class="tool-btn" id="btn-doctor"><span class="tool-glyph">${iconImg("shield")}</span>${esc(t("system_doctor"))}</button>
          <div class="rule"></div>
          <p class="tiny-title">${esc(t("collections"))}</p>
          ${collections}
          <div class="rule"></div>
          <p class="tiny-title">${esc(t("index_path"))}</p>
          <div class="path-row">
            <div class="path-chip">
              <span class="mono path-value">${esc(s.app.root || "-")}</span>
            </div>
            <button class="path-next" id="btn-root" title="${esc(t("select_folder"))}">${iconImg("folder-open")}</button>
          </div>
          <div class="rule"></div>
          <p class="tiny-title">${esc(t("advanced_search"))}</p>
          <input class="adv-chip mono" id="adv-regex" placeholder="${esc(t("regex_ph"))}" value="${esc(s.sidebar.regex || "")}" />
          <input class="adv-chip mono" id="adv-path" placeholder="${esc(t("path_prefix_ph"))}" value="${esc(s.sidebar.path_prefix || "")}" />
          <input class="adv-chip mono" id="adv-limit" type="number" min="1" max="200" value="${Number(s.sidebar.limit || 20)}" />
          <div class="toggle-row">
            <span class="toggle-pill ${s.sidebar.show_snippets !== false ? "on" : "off"}" id="adv-snippets"></span>
            <span>${esc(t("show_text_snippets"))}</span>
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
          shell.lang(),
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
        shell.state.app.status = shell.t("expanding_search", { limit: effectiveLimit(shell.state) });
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
          shell.state.app.status = shell.t("expanding_search", { limit: effectiveLimit(shell.state) });
          shell.paint();
          shell.scheduleSearchRefresh(0);
        }
      });
    }
    if (snippetsToggle) {
      snippetsToggle.addEventListener("click", () => {
        shell.state.sidebar.show_snippets = !(shell.state.sidebar.show_snippets !== false);
        shell.paint();
        if (shell.state.sidebar.show_snippets !== false) {
          shell.scheduleSnippetHydration(shell._searchToken, shell.state.top.query || "");
        }
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
    const t = (key, vars = {}) => shell.t(key, vars);
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
        const hasSnippet = typeof r.snippet === "string" && r.snippet.length > 0;
        const snip = hasSnippet
          ? rowPos < 100
            ? `<div class="row-snippet">${markSnippet(r.snippet, s.top.query)}</div>`
            : `<div class="row-snippet">${esc(String(r.snippet).slice(0, 220))}</div>`
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
            <div class="meta">- ${esc(r.size || "-")} - ${esc(t("modified_label"))} ${esc(r.modified || "-")}</div>
          </div>
          <div class="rank">#${r.rank}</div>
        </article>`;
      })
      .join("");

    this.innerHTML = `
      <main class="center">
        <div class="results-head">
          <div class="head-title-wrap">
            <h2 class="results-title">${esc(t("results_title"))}</h2>
            <div class="results-sub">${esc(t("results_for", { query: s.top.query || "" }))}</div>
          </div>
          <span class="chip chip-purple">${esc(t("hits_label", { hits: s.results.total_hits }))}</span>
          <span class="chip chip-green">${ms}</span>
        </div>
        <section class="rows">
          ${
            rows ||
            `<article class="row row-empty"><div class="empty-block"><h3>${esc(t("no_results_title"))}</h3><p>${esc(t("no_results_body"))}</p></div></article>`
          }
        </section>
        ${
          remaining > 0
            ? `<button class="load-more" id="load-more-btn">${esc(t("load_more", { remaining }))}</button>`
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
    const t = (key, vars = {}) => shell.t(key, vars);
    const rp = s.right_panel || {};
    const preview = rp.tab !== "chat";
    const mode = rp.chat_mode === "local_model" ? "local" : "extractive";
    const path = esc(rp.selected_path || "-");
    const file = esc(rp.file_name || t("no_file_selected"));
    const fileType = esc(rp.file_type || "-");
    const size = esc(rp.size || "-");
    const created = esc(rp.created || "-");
    const modified = esc(rp.modified || "-");
    const snippet = rp.snippet ? markSnippet(rp.snippet, s.top.query) : t("no_snippet");
    const matchCount = Number(rp.match_count || 0);
    const previewMedia = isImageExt(rp.file_type)
      ? `<img class="preview-image" src="${esc(fileSrc(rp.selected_path))}" alt="${file}" loading="lazy" decoding="async" />`
      : `<div class="preview-icon">${extLabel(rp.file_type)}</div><div class="preview-text">${esc(t("no_preview"))}</div>`;

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
            <button class="tab ${preview ? "active" : ""}" id="tab-preview">${esc(t("preview_tab"))}</button>
            <button class="tab ${!preview ? "active" : ""}" id="tab-chat">${esc(t("ai_chat_tab"))}</button>
          </div>
          <button class="close" id="panel-close">x</button>
        </div>
        ${
          preview
            ? `
          <div class="right-scroll">
            <section class="right-block">
              <p class="tiny-title">${esc(t("preview_title"))}</p>
              <div class="preview-card">
                ${previewMedia}
              </div>
            </section>
            <section class="right-block">
              <p class="tiny-title">${esc(t("file_info"))}</p>
              <div class="info-grid">
                <div class="info-row"><span class="k">${esc(t("label_name"))}</span><span class="v">${file}</span></div>
                <div class="info-row"><span class="k">${esc(t("label_path"))}</span><span class="v mono">${path}</span></div>
                <div class="info-row"><span class="k">${esc(t("label_type"))}</span><span class="v"><span class="mini-badge">${fileType} | ${size}</span></span></div>
                <div class="info-row"><span class="k">${esc(t("label_created"))}</span><span class="v">${created}</span></div>
                <div class="info-row"><span class="k">${esc(t("label_modified"))}</span><span class="v">${modified}</span></div>
              </div>
            </section>
            <section class="right-block">
              <p class="tiny-title">${esc(t("actions"))}</p>
              <div class="action-grid">
                <button class="action-btn" data-action="open">${esc(t("action_open"))}</button>
                <button class="action-btn" data-action="open_at_match">${esc(t("action_open_at_match"))}</button>
                <button class="action-btn" data-action="open_with">${esc(t("action_open_with"))}</button>
                <button class="action-btn" data-action="folder">${esc(t("action_folder"))}</button>
                <button class="action-btn" data-action="copy_path">${esc(t("action_copy_path"))}</button>
                <button class="action-btn action-primary" id="ask-doc">${esc(t("action_ask_doc"))}</button>
              </div>
            </section>
            <section class="right-block">
              <p class="tiny-title">${esc(t("match_in_document"))}</p>
              <div class="snippet-box">
                <div class="snippet-meta">${esc(t("matches_found", { count: matchCount }))}</div>
                <div class="snippet-text">${snippet}</div>
              </div>
            </section>
            <section class="right-block">
              <p class="tiny-title">${esc(t("metrics_title"))}</p>
              <div class="metrics-list">
                <div class="metric-row"><span>${esc(t("metrics_results"))}</span><span>${s.results.total_hits}</span></div>
                <div class="metric-row"><span>${esc(t("metrics_search_time"))}</span><span>${s.results.took_ms == null ? "N/A" : `${s.results.took_ms}ms`}</span></div>
                <div class="metric-row"><span>${esc(t("metrics_indexed"))}</span><span>${s.top.hits || 0}</span></div>
                <div class="metric-row"><span>${esc(t("metrics_watch"))}</span><span>${s.top.watch_running ? "ON" : "OFF"}</span></div>
              </div>
            </section>
          </div>
        `
            : `
          <div class="right-scroll chat-layout">
            <div class="chat-mode">
              <button class="${mode === "extractive" ? "active" : ""}" id="mode-ext">${esc(t("chat_mode_extractive"))}</button>
              <button class="${mode === "local" ? "active" : ""}" id="mode-local">${esc(t("chat_mode_local"))}</button>
            </div>
            <div class="chat-doc">
              <div class="file-icon ${extClass(rp.file_type)}">${extLabel(rp.file_type)}</div>
              <div>
                <div class="chat-doc-name">${file}</div>
                <div class="chat-doc-path mono">${path}</div>
              </div>
            </div>
            <div class="chat-quick">
              <button class="quick active" data-quick="summary">${esc(t("quick_summary"))}</button>
              <button class="quick" data-quick="key_dates">${esc(t("quick_key_dates"))}</button>
              <button class="quick" data-quick="main_topic">${esc(t("quick_main_topic"))}</button>
            </div>
            <div class="chat-feed">
              <div class="chat-bubble system">${esc(t("chat_ready_about", { file: rp.file_name || t("no_file_selected") }))}</div>
              ${messages || `<div class="chat-bubble assistant">${esc(t("chat_default_assistant"))}</div>`}
            </div>
            <div class="chat-composer">
              <textarea class="chat-input" placeholder="${esc(t("chat_input_placeholder"))}">${esc(rp.chat_input || "")}</textarea>
              <div class="chat-actions">
                <button class="send-btn"${rp.chat_busy ? " disabled" : ""}>${rp.chat_busy ? esc(t("sending")) : esc(t("send"))}</button>
                <button class="reset-btn" aria-label="${esc(t("reset"))}" title="${esc(t("reset"))}"${rp.chat_busy ? " disabled" : ""}>
                  <svg class="reset-icon-svg" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none" aria-hidden="true">
                    <g stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="3">
                      <path d="M5.185 31.954C6.529 26.914 10.638 23 15.854 23c4.895 0 8.164 4.425 8.056 9.32l-.057 2.569a7 7 0 0 0 2.097 5.154l1.106 1.086c1.586 1.557.66 4.224-1.555 4.408c-2.866.237-6.41.463-9.501.463c-3.982 0-7.963-.375-10.45-.666c-1.472-.172-2.558-1.428-2.417-2.902c.32-3.363 1.174-7.188 2.052-10.478"/>
                      <path d="M20 24.018c1.68-6.23 3.462-12.468 4.853-18.773c.219-.993-.048-2.01-1-2.365a8 8 0 0 0-.717-.226a8 8 0 0 0-.734-.162c-1.002-.17-1.742.578-2.048 1.547c-1.96 6.191-3.542 12.522-5.213 18.792M45 45H35m7-8H32m7-8H29m-18.951 8.75c-.167 1.5 0 5.2 2 8m5-7.75s0 5 2.951 7.5"/>
                    </g>
                  </svg>
                </button>
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
