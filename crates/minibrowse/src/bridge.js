// ─────────────────────────────────────────────────────────────────────────────
//  minibrowse Agentic In-Page Bridge (Web + Research + Flutter Semantics)
//  - Streams console, uncaught exceptions, and 4xx/5xx/CORS network events to Rust
//  - Enforces declarative mock routes & ad/tracker blocking on fetch / XHR
//  - Auto-activates Flutter Web Semantics (<flt-semantics-placeholder>)
//  - Computes Differential UI Trees (`computeUiDiff`) & Set-of-Mark overlays
//  - Extracts Information-Density Research Markdown (`extractResearch`)
//  - Diagnoses Vite/Next/React error overlays, WCAG contrast & CSS overflow root causes (`auditWebDebug`)
// ─────────────────────────────────────────────────────────────────────────────
(function () {
  if (window.__minibrowse) return;

  function postBridgeEvent(kind, payload) {
    try {
      if (
        window.webkit &&
        window.webkit.messageHandlers &&
        window.webkit.messageHandlers.minibrowseBridge
      ) {
        window.webkit.messageHandlers.minibrowseBridge.postMessage(
          JSON.stringify({ kind: kind, payload: payload })
        );
      }
    } catch (_) {}
  }

  // Full stealth fingerprint hygiene (inspired by `obscura --stealth`)
  (function applyStealthHygiene() {
    try {
      var navProto =
        typeof Navigator !== "undefined" && Navigator.prototype
          ? Navigator.prototype
          : navigator;
      Object.defineProperty(navProto, "webdriver", {
        get: function () {
          return false;
        },
        configurable: true
      });
      if (!navigator.languages || navigator.languages.length === 0) {
        Object.defineProperty(navProto, "languages", {
          get: function () {
            return ["en-US", "en"];
          },
          configurable: true
        });
      }
      if (!navigator.hardwareConcurrency || navigator.hardwareConcurrency <= 1) {
        Object.defineProperty(navProto, "hardwareConcurrency", {
          get: function () {
            return 8;
          },
          configurable: true
        });
      }
      if (!("deviceMemory" in navigator)) {
        Object.defineProperty(navProto, "deviceMemory", {
          get: function () {
            return 8;
          },
          configurable: true
        });
      }
      if (!navigator.plugins || navigator.plugins.length === 0) {
        var fakePlugins = [
          { name: "PDF Viewer", filename: "internal-pdf-viewer", description: "Portable Document Format" },
          { name: "Chrome PDF Viewer", filename: "internal-pdf-viewer", description: "Portable Document Format" },
          { name: "WebKit built-in PDF", filename: "internal-pdf-viewer", description: "Portable Document Format" }
        ];
        Object.defineProperty(navProto, "plugins", {
          get: function () {
            return fakePlugins;
          },
          configurable: true
        });
      }
      if (typeof window.chrome === "undefined") {
        window.chrome = { runtime: {}, app: { isInstalled: false } };
      }
      if (window.outerWidth === 0) {
        Object.defineProperty(window, "outerWidth", {
          get: function () {
            return window.innerWidth || 1440;
          },
          configurable: true
        });
      }
      if (window.outerHeight === 0) {
        Object.defineProperty(window, "outerHeight", {
          get: function () {
            return (window.innerHeight || 900) + 72;
          },
          configurable: true
        });
      }
      if (
        typeof Permissions !== "undefined" &&
        Permissions.prototype &&
        typeof Permissions.prototype.query === "function"
      ) {
        var origPermQuery = Permissions.prototype.query;
        Permissions.prototype.query = function (desc) {
          if (desc && desc.name === "notifications") {
            var st =
              typeof Notification !== "undefined" && Notification.permission
                ? Notification.permission
                : "default";
            return Promise.resolve({
              state: st === "default" ? "prompt" : st,
              onchange: null
            });
          }
          return origPermQuery.apply(this, arguments);
        };
      }
      function patchWebGlVendor(ctxCtor) {
        if (!ctxCtor || !ctxCtor.prototype || !ctxCtor.prototype.getParameter) return;
        var origGetParam = ctxCtor.prototype.getParameter;
        ctxCtor.prototype.getParameter = function (param) {
          if (param === 37445) return "Intel Inc.";
          if (param === 37446) return "Intel Iris OpenGL Engine";
          return origGetParam.apply(this, arguments);
        };
      }
      if (typeof WebGLRenderingContext !== "undefined") patchWebGlVendor(WebGLRenderingContext);
      if (typeof WebGL2RenderingContext !== "undefined") patchWebGlVendor(WebGL2RenderingContext);
    } catch (_) {}
  })();

  var state = {
    mockRoutes: [],
    blockedPatterns: [],
    adBlockEnabled: false,
    networkLog: [],
    apiTraffic: [],
    consoleLog: [],
    dialogLog: [],
    dialogAccept: true,
    dialogPromptText: null,
    prevSnapshot: null,
    selectorMap: [],
    selectorWaiters: [],
    pointerMode: "auto",
    inflightRequests: 0,
    lastMutationTs: Date.now()
  };

  // Non-blocking trap for synchronous JS dialogs so alert/confirm/prompt never stall headless automation
  (function installDialogTraps() {
    window.alert = function (msg) {
      var text = String(msg === undefined ? "" : msg);
      state.dialogLog.push({ type: "alert", message: text, handled: true });
      pushConsole("info", "[dialog:alert] " + text);
    };
    window.confirm = function (msg) {
      var text = String(msg === undefined ? "" : msg);
      state.dialogLog.push({
        type: "confirm",
        message: text,
        accepted: state.dialogAccept
      });
      pushConsole("info", "[dialog:confirm] " + text);
      return Boolean(state.dialogAccept);
    };
    window.prompt = function (msg, defaultVal) {
      var text = String(msg === undefined ? "" : msg);
      var out = state.dialogAccept
        ? state.dialogPromptText !== null
          ? state.dialogPromptText
          : String(defaultVal || "")
        : null;
      state.dialogLog.push({ type: "prompt", message: text, response: out });
      pushConsole("info", "[dialog:prompt] " + text);
      return out;
    };
    window.print = function () {
      pushConsole("info", "[dialog:print] suppressed interactive print modal");
    };
  })();

  // Sorted tracker suffix list searched in O(log N) via binarySearchSuffix
  var AD_TRACKER_SUFFIXES = [
    "2mdn.net",
    "adnxs.com",
    "ads-twitter.com",
    "adsafeprotected.com",
    "adsrvr.org",
    "adsystem.com",
    "amazon-adsystem.com",
    "amplitude.com",
    "analytics.tiktok.com",
    "appsflyer.com",
    "bat.bing.com",
    "branch.io",
    "braze.com",
    "bugsnag.com",
    "chartbeat.com",
    "chartbeat.net",
    "clarity.ms",
    "clicktale.net",
    "cloudflareinsights.com",
    "comscore.com",
    "connect.facebook.net",
    "criteo.com",
    "criteo.net",
    "ct.pinterest.com",
    "datadoghq-browser-agent.com",
    "demdex.net",
    "doubleclick.net",
    "facebook.net",
    "fastclick.net",
    "fingerprintjs.com",
    "fpjs.io",
    "fullstory.com",
    "go-mpulse.net",
    "google-analytics.com",
    "googleadservices.com",
    "googlesyndication.com",
    "googletagmanager.com",
    "googletagservices.com",
    "heapanalytics.com",
    "hotjar.com",
    "hotjar.io",
    "hs-analytics.net",
    "hs-scripts.com",
    "hsadspixel.net",
    "inspectlet.com",
    "kissmetrics.com",
    "krxd.net",
    "loggly.com",
    "logrocket.com",
    "logrocket.io",
    "luckyorange.com",
    "marketo.net",
    "mathtag.com",
    "media.net",
    "mixpanel.com",
    "moatads.com",
    "mouseflow.com",
    "mparticle.com",
    "mxpnl.com",
    "newrelic.com",
    "nr-data.net",
    "omtrdc.net",
    "onesignal.com",
    "openx.net",
    "optimizely.com",
    "outbrain.com",
    "pardot.com",
    "parsely.com",
    "pendo.io",
    "posthog.com",
    "pubmatic.com",
    "px.ads.linkedin.com",
    "quantcast.com",
    "quantserve.com",
    "rollbar.com",
    "rubiconproject.com",
    "rudderlabs.com",
    "scorecardresearch.com",
    "segment.com",
    "segment.io",
    "sentry-cdn.com",
    "sharethrough.com",
    "smartadserver.com",
    "snap.licdn.com",
    "statcounter.com",
    "taboola.com",
    "tapad.com",
    "tealiumiq.com",
    "treasuredata.com",
    "turn.com",
    "visualwebsiteoptimizer.com",
    "vwo.com",
    "walkme.com",
    "yieldmo.com"
  ];

  function binarySearchStr(sortedArr, target) {
    var lo = 0;
    var hi = sortedArr.length - 1;
    while (lo <= hi) {
      var mid = (lo + hi) >> 1;
      var cur = sortedArr[mid];
      if (cur === target) return true;
      if (cur < target) lo = mid + 1;
      else hi = mid - 1;
    }
    return false;
  }

  function isPrivateHostName(host) {
    var h = String(host || "").trim().toLowerCase();
    if (!h) return false;
    if (
      h === "localhost" ||
      h === "127.0.0.1" ||
      h === "[::1]" ||
      h === "::1" ||
      h === "0.0.0.0" ||
      h === "169.254.169.254" ||
      h === "metadata.google.internal" ||
      h.indexOf("127.") === 0 ||
      h.indexOf("10.") === 0 ||
      h.indexOf("192.168.") === 0 ||
      h.indexOf("169.254.") === 0
    ) {
      return true;
    }
    if (h.indexOf("172.") === 0) {
      var second = parseInt(h.split(".")[1] || "0", 10);
      if (second >= 16 && second <= 31) return true;
    }
    return false;
  }

  function isTrackerOrSsrfBlocked(rawUrl) {
    var parsed;
    try {
      parsed = new URL(String(rawUrl || ""), location.href);
    } catch (_) {
      return false;
    }
    var pageHost = String(location.hostname || "").toLowerCase();
    var targetHost = String(parsed.hostname || "").toLowerCase();
    // Block cross-origin public -> private/loopback/metadata SSRF probes
    if (
      pageHost &&
      !isPrivateHostName(pageHost) &&
      isPrivateHostName(targetHost)
    ) {
      return true;
    }
    if (state.adBlockEnabled && targetHost) {
      var rest = targetHost;
      while (rest) {
        if (binarySearchStr(AD_TRACKER_SUFFIXES, rest)) return true;
        var dot = rest.indexOf(".");
        if (dot === -1) break;
        var next = rest.slice(dot + 1);
        if (next.indexOf(".") === -1) break;
        rest = next;
      }
    }
    return false;
  }

  function matchesUrlPattern(pattern, url) {
    if (!pattern || pattern === "*") return true;
    var p = String(pattern).toLowerCase();
    var u = String(url || "").toLowerCase();
    var parts = p.split("*");
    var pos = 0;
    for (var i = 0; i < parts.length; i++) {
      var seg = parts[i];
      if (!seg) continue;
      var found = u.indexOf(seg, pos);
      if (found === -1) return false;
      pos = found + seg.length;
    }
    return true;
  }

  function isBlockedUrl(url) {
    if (isTrackerOrSsrfBlocked(url)) return true;
    var u = String(url || "").toLowerCase();
    for (var i = 0; i < state.blockedPatterns.length; i++) {
      if (matchesUrlPattern(state.blockedPatterns[i], u)) return true;
    }
    return false;
  }

  function findMockRoute(url) {
    for (var i = 0; i < state.mockRoutes.length; i++) {
      var r = state.mockRoutes[i];
      if (matchesUrlPattern(r.pattern, url)) return r;
    }
    return null;
  }

  function pushConsole(level, text) {
    var entry = { level: level, text: String(text || ""), ts: Date.now() };
    if (state.consoleLog.length >= 200) state.consoleLog.shift();
    state.consoleLog.push(entry);
    if (!window.__minicode_console) window.__minicode_console = [];
    window.__minicode_console.push("[" + level + "] " + entry.text);
    postBridgeEvent("console", entry);
  }

  var SECRET_FIELD_SUBSTRINGS = [
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "authorization",
    "client_secret"
  ];

  function isSecretKeyName(keyName) {
    var lower = String(keyName || "").toLowerCase();
    for (var i = 0; i < SECRET_FIELD_SUBSTRINGS.length; i++) {
      if (lower.indexOf(SECRET_FIELD_SUBSTRINGS[i]) !== -1) return true;
    }
    return false;
  }

  function redactJsonSecrets(val, depth) {
    if (depth > 4 || val === null || val === undefined) return val;
    if (Array.isArray(val)) {
      var outArr = [];
      for (var i = 0; i < val.length && i < 25; i++) {
        outArr.push(redactJsonSecrets(val[i], depth + 1));
      }
      return outArr;
    }
    if (typeof val === "object") {
      var outObj = {};
      var keys = Object.keys(val);
      for (var k = 0; k < keys.length && k < 40; k++) {
        var key = keys[k];
        outObj[key] = isSecretKeyName(key)
          ? "[REDACTED]"
          : redactJsonSecrets(val[key], depth + 1);
      }
      return outObj;
    }
    return val;
  }

  function sanitizeBodyPreview(rawBody, maxLen) {
    if (rawBody === null || rawBody === undefined) return null;
    var limit = maxLen || 1600;
    var text = "";
    try {
      text = typeof rawBody === "string" ? rawBody : JSON.stringify(rawBody);
    } catch (_) {
      return "[non-serializable payload]";
    }
    if (!text) return null;
    var trimmed = text.trim();
    if (trimmed.charAt(0) === "{" || trimmed.charAt(0) === "[") {
      try {
        var parsed = JSON.parse(trimmed);
        text = JSON.stringify(redactJsonSecrets(parsed, 0));
      } catch (_) {}
    }
    return text.length > limit ? text.slice(0, limit) + "...[truncated]" : text;
  }

  function isStaticAssetPath(urlStr) {
    var clean = String(urlStr || "").split("?")[0].toLowerCase();
    var exts = [".css", ".png", ".jpg", ".jpeg", ".webp", ".svg", ".woff", ".woff2", ".ttf", ".ico", ".gif"];
    for (var i = 0; i < exts.length; i++) {
      if (
        clean.length >= exts[i].length &&
        clean.slice(clean.length - exts[i].length) === exts[i]
      ) {
        return true;
      }
    }
    return false;
  }

  function pushApiTraffic(entry) {
    if (!entry || isStaticAssetPath(entry.url)) return;
    if (state.apiTraffic.length >= 40) state.apiTraffic.shift();
    state.apiTraffic.push(entry);
  }

  function pushNetwork(entry) {
    if (state.networkLog.length >= 150) state.networkLog.shift();
    state.networkLog.push(entry);
    if (!entry.ok || entry.status >= 400) {
      postBridgeEvent("network_error", entry);
    }
  }

  ["log", "info", "warn", "error", "debug"].forEach(function (lvl) {
    var orig = console[lvl] ? console[lvl].bind(console) : function () {};
    console[lvl] = function () {
      try {
        var text = Array.prototype.map
          .call(arguments, function (a) {
            try {
              return typeof a === "object" ? JSON.stringify(a) : String(a);
            } catch (_) {
              return String(a);
            }
          })
          .join(" ");
        pushConsole(lvl, text);
      } catch (_) {}
      orig.apply(null, arguments);
    };
  });

  window.addEventListener("error", function (ev) {
    var msg =
      (ev && (ev.message || (ev.error && ev.error.stack))) || "Runtime error";
    var loc =
      ev && ev.filename ? " (" + ev.filename + ":" + (ev.lineno || 0) + ")" : "";
    pushConsole("error", msg + loc);
  });

  window.addEventListener("unhandledrejection", function (ev) {
    var reason =
      ev && ev.reason
        ? ev.reason.stack || ev.reason.message || String(ev.reason)
        : "Unhandled Promise rejection";
    pushConsole("error", reason);
  });

  // Intercept fetch for network diagnostics, ad-blocking, and declarative mock routes
  if (typeof window.fetch === "function") {
    var origFetch = window.fetch.bind(window);
    window.fetch = function (input, init) {
      var method = String(
        (init && init.method) || (input && input.method) || "GET"
      ).toUpperCase();
      var url =
        typeof input === "string"
          ? input
          : (input && input.url) || String(input);
      if (isBlockedUrl(url)) {
        pushNetwork({
          method: method,
          url: url,
          status: 0,
          ok: false,
          blocked: true,
          duration_ms: 0,
          error: "BlockedByMiniBrowse"
        });
        return Promise.reject(new TypeError("Blocked by MiniBrowse policy"));
      }
      var reqPreview = sanitizeBodyPreview(init && init.body ? init.body : null, 1200);
      var mock = findMockRoute(url);
      if (mock) {
        var ctype = mock.content_type || "application/json";
        pushNetwork({
          method: method,
          url: url,
          status: mock.status || 200,
          ok: (mock.status || 200) < 400,
          mocked: true,
          duration_ms: 0
        });
        pushApiTraffic({
          transport: "fetch",
          method: method,
          url: url,
          status: mock.status || 200,
          ok: (mock.status || 200) < 400,
          mocked: true,
          duration_ms: 0,
          content_type: ctype,
          request_body: reqPreview,
          response_body: sanitizeBodyPreview(mock.body || "", 1600)
        });
        return Promise.resolve(
          new Response(mock.body || "", {
            status: mock.status || 200,
            headers: {
              "Content-Type": ctype,
              "Access-Control-Allow-Origin": "*"
            }
          })
        );
      }
      var t0 = Date.now();
      state.inflightRequests++;
      return origFetch(input, init).then(
        function (resp) {
          state.inflightRequests = Math.max(0, state.inflightRequests - 1);
          state.lastMutationTs = Date.now();
          var dur = Date.now() - t0;
          var respType = String(
            (resp.headers && resp.headers.get && resp.headers.get("content-type")) || ""
          ).toLowerCase();
          var shouldSampleBody =
            !resp.ok ||
            respType.indexOf("json") !== -1 ||
            respType.indexOf("text/") !== -1 ||
            respType.indexOf("graphql") !== -1;
          if (shouldSampleBody && resp.clone) {
            try {
              resp
                .clone()
                .text()
                .then(function (bodyText) {
                  var respPreview = sanitizeBodyPreview(bodyText, 1800);
                  pushApiTraffic({
                    transport: "fetch",
                    method: method,
                    url: url,
                    status: resp.status,
                    ok: resp.ok,
                    duration_ms: dur,
                    content_type: respType,
                    request_body: reqPreview,
                    response_body: respPreview
                  });
                  if (!resp.ok) {
                    pushNetwork({
                      method: method,
                      url: url,
                      status: resp.status,
                      ok: false,
                      duration_ms: dur,
                      error: String(bodyText || resp.statusText || "").slice(0, 240)
                    });
                  }
                })
                .catch(function () {
                  if (!resp.ok) {
                    pushNetwork({
                      method: method,
                      url: url,
                      status: resp.status,
                      ok: false,
                      duration_ms: dur,
                      error: resp.statusText
                    });
                  }
                });
            } catch (_) {}
          }
          if (resp.ok) {
            pushNetwork({
              method: method,
              url: url,
              status: resp.status,
              ok: true,
              duration_ms: dur
            });
          }
          return resp;
        },
        function (err) {
          state.inflightRequests = Math.max(0, state.inflightRequests - 1);
          pushNetwork({
            method: method,
            url: url,
            status: 0,
            ok: false,
            duration_ms: Date.now() - t0,
            error: String(err)
          });
          throw err;
        }
      );
    };
  }

  // Intercept XMLHttpRequest for inflight networkIdle tracking, ad/SSRF blocking, and error logging
  if (typeof window.XMLHttpRequest === "function") {
    var origXhrOpen = window.XMLHttpRequest.prototype.open;
    var origXhrSend = window.XMLHttpRequest.prototype.send;
    window.XMLHttpRequest.prototype.open = function (method, url) {
      this.__mbMethod = String(method || "GET").toUpperCase();
      this.__mbUrl = String(url || "");
      return origXhrOpen.apply(this, arguments);
    };
    window.XMLHttpRequest.prototype.send = function () {
      var xhr = this;
      var url = xhr.__mbUrl || "";
      var method = xhr.__mbMethod || "GET";
      if (isBlockedUrl(url)) {
        pushNetwork({
          method: method,
          url: url,
          status: 0,
          ok: false,
          blocked: true,
          duration_ms: 0,
          error: "BlockedByMiniBrowse"
        });
        try {
          xhr.abort();
        } catch (_) {}
        return;
      }
      var reqBody = arguments.length > 0 ? sanitizeBodyPreview(arguments[0], 1200) : null;
      var t0 = Date.now();
      state.inflightRequests++;
      var settled = false;
      function onDone() {
        if (settled) return;
        settled = true;
        state.inflightRequests = Math.max(0, state.inflightRequests - 1);
        state.lastMutationTs = Date.now();
        var st = xhr.status || 0;
        var dur = Date.now() - t0;
        var respPreview = null;
        try {
          if (!xhr.responseType || xhr.responseType === "text") {
            respPreview = sanitizeBodyPreview(xhr.responseText, 1800);
          } else if (xhr.responseType === "json") {
            respPreview = sanitizeBodyPreview(xhr.response, 1800);
          }
        } catch (_) {}
        pushApiTraffic({
          transport: "xhr",
          method: method,
          url: url,
          status: st,
          ok: st >= 200 && st < 400,
          duration_ms: dur,
          content_type: String((xhr.getResponseHeader && xhr.getResponseHeader("content-type")) || ""),
          request_body: reqBody,
          response_body: respPreview
        });
        pushNetwork({
          method: method,
          url: url,
          status: st,
          ok: st >= 200 && st < 400,
          duration_ms: dur,
          error: st >= 400 ? String(xhr.statusText || "HTTP " + st) : undefined
        });
      }
      xhr.addEventListener("loadend", onDone);
      xhr.addEventListener("error", onDone);
      xhr.addEventListener("abort", onDone);
      return origXhrSend.apply(this, arguments);
    };
  }

  function queryAcrossFramesAndShadow(selector) {
    if (!selector) return null;
    try {
      var topHit = document.querySelector(selector);
      if (topHit) return topHit;
    } catch (_) {
      return null;
    }
    var docs = collectAccessibleDocuments();
    for (var d = 0; d < docs.length; d++) {
      var curDoc = docs[d].doc;
      try {
        var found = curDoc.querySelector(selector);
        if (found) return found;
      } catch (_) {}
      var all = curDoc.querySelectorAll ? curDoc.querySelectorAll("*") : [];
      for (var i = 0; i < all.length; i++) {
        if (all[i].shadowRoot) {
          try {
            var sHit = all[i].shadowRoot.querySelector(selector);
            if (sHit) return sHit;
          } catch (_) {}
        }
      }
    }
    return null;
  }

  function checkSelectorCondition(selector, stateMode) {
    var el = queryAcrossFramesAndShadow(selector);
    if (stateMode === "detached" || stateMode === "hidden") {
      if (!el) return true;
      if (stateMode === "detached") return false;
      var csH = window.getComputedStyle(el);
      return csH.display === "none" || csH.visibility === "hidden" || csH.opacity === "0";
    }
    if (!el) return false;
    if (stateMode === "visible") {
      var cs = window.getComputedStyle(el);
      if (cs.display === "none" || cs.visibility === "hidden" || cs.opacity === "0") {
        return false;
      }
      var r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0;
    }
    return true;
  }

  function notifySelectorWaiters() {
    if (!state.selectorWaiters || state.selectorWaiters.length === 0) return;
    var remaining = [];
    for (var i = 0; i < state.selectorWaiters.length; i++) {
      var w = state.selectorWaiters[i];
      if (checkSelectorCondition(w.selector, w.mode)) {
        clearTimeout(w.timer);
        w.resolve({
          matched: true,
          selector: w.selector,
          state: w.mode,
          waited_ms: Date.now() - w.start
        });
      } else {
        remaining.push(w);
      }
    }
    state.selectorWaiters = remaining;
  }

  // Track DOM mutation quiescence for sub-30ms event-driven waitForIdle & 0ms waitForSelector
  function installMutationTracker() {
    if (state.mutationObserver || typeof MutationObserver !== "function") return;
    var target = document.documentElement || document.body;
    if (!target) return;
    state.mutationObserver = new MutationObserver(function () {
      state.lastMutationTs = Date.now();
      notifySelectorWaiters();
    });
    try {
      state.mutationObserver.observe(target, {
        childList: true,
        subtree: true,
        attributes: true
      });
    } catch (_) {}
  }
  installMutationTracker();
  window.addEventListener("DOMContentLoaded", function () {
    installMutationTracker();
    notifySelectorWaiters();
  }, { once: true });

  function waitForSelector(selector, stateMode, timeoutMs) {
    installMutationTracker();
    var mode = String(stateMode || "attached").toLowerCase();
    var maxMs = typeof timeoutMs === "number" && timeoutMs > 0 ? timeoutMs : 5000;
    var start = Date.now();
    if (checkSelectorCondition(selector, mode)) {
      return Promise.resolve({
        matched: true,
        selector: selector,
        state: mode,
        waited_ms: 0
      });
    }
    return new Promise(function (resolve) {
      var entry = {
        selector: selector,
        mode: mode,
        start: start,
        resolve: resolve,
        timer: null
      };
      entry.timer = setTimeout(function () {
        var next = [];
        for (var i = 0; i < state.selectorWaiters.length; i++) {
          if (state.selectorWaiters[i] !== entry) next.push(state.selectorWaiters[i]);
        }
        state.selectorWaiters = next;
        var okNow = checkSelectorCondition(selector, mode);
        resolve({
          matched: okNow,
          selector: selector,
          state: mode,
          waited_ms: Date.now() - start
        });
      }, maxMs);
      state.selectorWaiters.push(entry);
    });
  }

  function waitForIdle(quietMs, timeoutMs) {
    installMutationTracker();
    var qMs = typeof quietMs === "number" && quietMs >= 0 ? quietMs : 25;
    var maxMs = typeof timeoutMs === "number" && timeoutMs > 0 ? timeoutMs : 600;
    var start = Date.now();
    return new Promise(function (resolve) {
      function check() {
        var now = Date.now();
        var quietFor = now - state.lastMutationTs;
        if (state.inflightRequests === 0 && quietFor >= qMs) {
          resolve({
            idle: true,
            inflight: 0,
            waited_ms: now - start
          });
          return;
        }
        if (now - start >= maxMs) {
          resolve({
            idle: false,
            inflight: state.inflightRequests,
            waited_ms: now - start
          });
          return;
        }
        setTimeout(check, 12);
      }
      if (typeof requestAnimationFrame === "function") {
        requestAnimationFrame(check);
      } else {
        setTimeout(check, 8);
      }
    });
  }

  // Traverse main document + accessible same-origin <iframe> documents (depth <= 2, max 12 frames)
  function collectAccessibleDocuments() {
    var out = [{ doc: document, win: window, offsetX: 0, offsetY: 0 }];
    var queue = [{ doc: document, depth: 0, baseX: 0, baseY: 0 }];
    while (queue.length > 0 && out.length < 12) {
      var cur = queue.shift();
      if (cur.depth >= 2) continue;
      var frames;
      try {
        frames = cur.doc.querySelectorAll("iframe, frame");
      } catch (_) {
        frames = [];
      }
      for (var i = 0; i < frames.length && out.length < 12; i++) {
        var fr = frames[i];
        try {
          var subDoc = fr.contentDocument;
          var subWin = fr.contentWindow;
          if (subDoc && subDoc.body) {
            var rect = fr.getBoundingClientRect();
            var ox = cur.baseX + rect.left;
            var oy = cur.baseY + rect.top;
            out.push({ doc: subDoc, win: subWin || window, offsetX: ox, offsetY: oy });
            queue.push({ doc: subDoc, depth: cur.depth + 1, baseX: ox, baseY: oy });
          }
        } catch (_) {}
      }
    }
    return out;
  }

  function isAsciiSpace(code) {
    return code === 32 || code === 9 || code === 10 || code === 13 || code === 12;
  }

  function splitAsciiWords(str) {
    var s = String(str || "");
    var out = [];
    var cur = "";
    for (var i = 0; i < s.length; i++) {
      if (isAsciiSpace(s.charCodeAt(i))) {
        if (cur) {
          out.push(cur);
          cur = "";
        }
      } else {
        cur += s.charAt(i);
      }
    }
    if (cur) out.push(cur);
    return out;
  }

  function collapseWhitespace(str) {
    return splitAsciiWords(str).join(" ");
  }

  function splitColorTokens(str) {
    var s = String(str || "");
    var out = [];
    var cur = "";
    for (var i = 0; i < s.length; i++) {
      var code = s.charCodeAt(i);
      if (isAsciiSpace(code) || code === 44 || code === 47) {
        if (cur) {
          out.push(cur);
          cur = "";
        }
      } else {
        cur += s.charAt(i);
      }
    }
    if (cur) out.push(cur);
    return out;
  }

  // Relative luminance & WCAG 2.1 contrast ratio (pure photometric formula)
  function parseRgbComponents(cssColor) {
    if (!cssColor) return null;
    var s = String(cssColor).trim().toLowerCase();
    var open = s.indexOf("(");
    var close = s.lastIndexOf(")");
    if (s.indexOf("rgb") !== 0 || open === -1 || close === -1) return null;
    var parts = splitColorTokens(s.slice(open + 1, close));
    if (parts.length < 3) return null;
    var r = parseFloat(parts[0]);
    var g = parseFloat(parts[1]);
    var b = parseFloat(parts[2]);
    var a = parts.length >= 4 ? parseFloat(parts[3]) : 1.0;
    if (isNaN(r) || isNaN(g) || isNaN(b)) return null;
    return [r, g, b, isNaN(a) ? 1.0 : a];
  }

  function srgbChannelLuminance(v255) {
    var c = Math.max(0, Math.min(255, v255)) / 255.0;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  }

  function relativeLuminance(rgb) {
    return (
      0.2126 * srgbChannelLuminance(rgb[0]) +
      0.7152 * srgbChannelLuminance(rgb[1]) +
      0.0722 * srgbChannelLuminance(rgb[2])
    );
  }

  function wcagContrastRatio(fg, bg) {
    var l1 = relativeLuminance(fg);
    var l2 = relativeLuminance(bg);
    var lighter = Math.max(l1, l2);
    var darker = Math.min(l1, l2);
    return (lighter + 0.05) / (darker + 0.05);
  }

  function resolveEffectiveBg(el) {
    var cur = el;
    while (cur && cur.nodeType === 1) {
      var cs = window.getComputedStyle(cur);
      var rgba = parseRgbComponents(cs.backgroundColor);
      if (rgba && rgba[3] > 0.15) return rgba;
      cur = cur.parentElement;
    }
    var bodyCs = document.body ? window.getComputedStyle(document.body) : null;
    var bodyRgba = bodyCs ? parseRgbComponents(bodyCs.backgroundColor) : null;
    return bodyRgba && bodyRgba[3] > 0.05 ? bodyRgba : [255, 255, 255, 1];
  }

  function shortSelector(el) {
    if (!el || el.nodeType !== 1) return "unknown";
    if (el.id) return "#" + el.id;
    var tag = el.tagName.toLowerCase();
    var cls =
      typeof el.className === "string"
        ? splitAsciiWords(el.className).slice(0, 2).join(".")
        : "";
    return cls ? tag + "." + cls : tag;
  }

  // Auto-activate Flutter Web Semantics placeholder when present
  function ensureFlutterSemantics() {
    var placeholder = document.querySelector("flt-semantics-placeholder");
    if (placeholder) {
      try {
        placeholder.click();
        return true;
      } catch (_) {}
    }
    return Boolean(document.querySelector("flt-semantics-host, flt-semantics"));
  }

  function formatTableMarkdown(tableEl) {
    var rows = tableEl.querySelectorAll ? tableEl.querySelectorAll("tr") : [];
    var lines = [];
    for (var r = 0; r < rows.length && lines.length < 60; r++) {
      var cells = rows[r].querySelectorAll("th, td");
      if (!cells || cells.length === 0) continue;
      var vals = [];
      for (var c = 0; c < cells.length; c++) {
        vals.push(
          collapseWhitespace(cells[c].innerText || cells[c].textContent || "")
            .split("|")
            .join("\\|")
        );
      }
      lines.push("| " + vals.join(" | ") + " |");
      if (lines.length === 1) {
        var sep = [];
        for (var s = 0; s < vals.length; s++) sep.push("---");
        lines.push("| " + sep.join(" | ") + " |");
      }
    }
    return lines.join("\n");
  }

  var TAG_MARKDOWN_FORMATTERS = {
    H1: function (t) { return "# " + t; },
    H2: function (t) { return "## " + t; },
    H3: function (t) { return "### " + t; },
    H4: function (t) { return "#### " + t; },
    PRE: function (t) { return "```\n" + t + "\n```"; },
    LI: function (t) { return "- " + t; },
    BLOCKQUOTE: function (t) { return "> " + t; }
  };

  // Compact hierarchical `semantic_tree` serializer (inspired by `moli fetch --dump semantic_tree_text`)
  function extractSemanticTree(maxChars) {
    var budget = typeof maxChars === "number" && maxChars > 0 ? maxChars : 10000;
    var lines = [];
    var used = 0;
    var SKIP = { SCRIPT: 1, STYLE: 1, NOSCRIPT: 1, SVG: 1, META: 1, LINK: 1 };
    var STRUCTURAL = {
      MAIN: 1, ARTICLE: 1, SECTION: 1, NAV: 1, HEADER: 1, FOOTER: 1, FORM: 1, DIALOG: 1,
      TABLE: 1, UL: 1, OL: 1, H1: 1, H2: 1, H3: 1, H4: 1, BUTTON: 1, A: 1, INPUT: 1,
      SELECT: 1, TEXTAREA: 1, SUMMARY: 1, LABEL: 1, P: 1, PRE: 1
    };

    function walkSem(node, depth) {
      if (!node || node.nodeType !== 1 || used >= budget || depth > 10) return;
      var tag = node.tagName;
      if (SKIP[tag]) return;
      if (node.hasAttribute("hidden") || node.getAttribute("aria-hidden") === "true") return;
      try {
        var cs = window.getComputedStyle(node);
        if (cs.display === "none" || cs.visibility === "hidden") return;
      } catch (_) {}

      var role = node.getAttribute("role") || "";
      var isInteractive =
        tag === "BUTTON" || tag === "A" || tag === "INPUT" || tag === "SELECT" ||
        tag === "TEXTAREA" || role === "button" || role === "link" || role === "tab" ||
        role === "checkbox" || role === "Dialog";
      var emitSelf = Boolean(STRUCTURAL[tag] || role || isInteractive);
      var nextDepth = depth;

      if (emitSelf) {
        var indent = "";
        for (var s = 0; s < depth; s++) indent += "  ";
        var head = role ? role : tag.toLowerCase();
        if (node.id) head += "#" + node.id;
        var label = "";
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") {
          var inpType = (node.getAttribute("type") || "").toLowerCase();
          var val = inpType === "password" ? "[REDACTED]" : String(node.value || "").slice(0, 60);
          var ph = node.getAttribute("placeholder") || node.getAttribute("aria-label") || node.getAttribute("name") || "";
          label = ph ? "\"" + collapseWhitespace(ph) + "\"" : "";
          if (val) label += (label ? " " : "") + "value=\"" + collapseWhitespace(val) + "\"";
        } else if (
          tag === "H1" || tag === "H2" || tag === "H3" || tag === "H4" ||
          tag === "BUTTON" || tag === "A" || tag === "P" || tag === "LABEL" || tag === "SUMMARY"
        ) {
          var ownTxt = collapseWhitespace(node.innerText || node.textContent || "").slice(0, 140);
          if (ownTxt) label = "\"" + ownTxt + "\"";
        } else if (node.getAttribute("aria-label")) {
          label = "\"" + collapseWhitespace(node.getAttribute("aria-label")).slice(0, 100) + "\"";
        }
        var line = indent + "- " + head + (label ? " " + label : "");
        if (used + line.length + 1 <= budget) {
          lines.push(line);
          used += line.length + 1;
        }
        nextDepth = depth + 1;
        if (tag === "P" || tag === "H1" || tag === "H2" || tag === "H3" || tag === "H4" || tag === "BUTTON" || tag === "A") {
          return;
        }
      }

      var children = node.children || [];
      for (var c = 0; c < children.length && used < budget; c++) {
        walkSem(children[c], nextDepth);
      }
      if (node.shadowRoot && node.shadowRoot.children) {
        for (var sc = 0; sc < node.shadowRoot.children.length && used < budget; sc++) {
          walkSem(node.shadowRoot.children[sc], nextDepth);
        }
      }
    }

    if (document.body) walkSem(document.body, 0);
    return lines.join("\n");
  }

  // Information-density Prune4Web research extractor (main document + same-origin iframes)
  function extractResearch(query, maxChars, format) {
    var budget = typeof maxChars === "number" && maxChars > 0 ? maxChars : 12000;
    if (String(format || "").toLowerCase() === "semantic_tree") {
      var treeText = extractSemanticTree(budget);
      return {
        url: location.href,
        title: document.title || "",
        format: "semantic_tree",
        blocks_extracted: treeText ? treeText.split("\n").length : 0,
        markdown: treeText,
        citations: []
      };
    }
    var qTerms = splitAsciiWords(String(query || "").toLowerCase()).filter(
      function (w) {
        return w.length >= 2;
      }
    );

    var NOISE_TAGS = {
      SCRIPT: 1,
      STYLE: 1,
      NOSCRIPT: 1,
      SVG: 1,
      NAV: 1,
      FOOTER: 1,
      ASIDE: 1,
      IFRAME: 1
    };

    var blocks = [];
    var citations = [];
    var seenHref = {};
    var docs = collectAccessibleDocuments();

    for (var d = 0; d < docs.length; d++) {
      var curDoc = docs[d].doc;
      var root =
        curDoc.querySelector("article, main, [role='main']") || curDoc.body;
      if (!root) continue;

      var candidates = root.querySelectorAll(
        "h1, h2, h3, h4, p, pre, li, table, blockquote"
      );
      for (var i = 0; i < candidates.length; i++) {
        var node = candidates[i];
        var skip = false;
        var anc = node;
        while (anc && anc !== root) {
          if (NOISE_TAGS[anc.tagName]) {
            skip = true;
            break;
          }
          if (anc !== node && anc.tagName === "TABLE") {
            skip = true;
            break;
          }
          var role = (anc.getAttribute && anc.getAttribute("role")) || "";
          if (
            role === "navigation" ||
            role === "banner" ||
            role === "contentinfo" ||
            role === "dialog"
          ) {
            skip = true;
            break;
          }
          anc = anc.parentElement;
        }
        if (skip) continue;

        var tag = node.tagName;
        var text =
          tag === "TABLE"
            ? formatTableMarkdown(node)
            : String(node.innerText || node.textContent || "").trim();
        if (!text || text.length < 3) continue;

        var formatter = TAG_MARKDOWN_FORMATTERS[tag];
        var formatted = formatter ? formatter(text) : text;

        var score = 1.0;
        if (qTerms.length > 0) {
          var lower = text.toLowerCase();
          var hits = 0;
          for (var q = 0; q < qTerms.length; q++) {
            if (lower.indexOf(qTerms[q]) !== -1) hits++;
          }
          score =
            tag.indexOf("H") === 0
              ? 1.5 + hits * 2.0
              : tag === "PRE" || tag === "TABLE"
                ? 1.2 + hits * 2.5
                : hits > 0
                  ? 1.0 + hits * 2.0
                  : 0.15;
        }

        blocks.push({ idx: blocks.length, text: formatted, score: score });

        var links = node.querySelectorAll ? node.querySelectorAll("a[href]") : [];
        for (var l = 0; l < links.length && citations.length < 30; l++) {
          var href = links[l].href;
          var ltxt = String(links[l].innerText || "").trim();
          if (
            href &&
            href.indexOf("http") === 0 &&
            ltxt.length >= 2 &&
            !seenHref[href]
          ) {
            seenHref[href] = 1;
            citations.push({ title: ltxt.slice(0, 90), url: href });
          }
        }
      }
    }

    if (qTerms.length > 0) {
      blocks = blocks.filter(function (b) {
        return b.score >= 1.0;
      });
    }

    var mdParts = [];
    var used = 0;
    for (var b = 0; b < blocks.length; b++) {
      var piece = blocks[b].text;
      if (used + piece.length > budget) break;
      mdParts.push(piece);
      used += piece.length + 2;
    }

    return {
      url: location.href,
      title: document.title || "",
      blocks_extracted: mdParts.length,
      markdown: mdParts.join("\n\n"),
      citations: citations
    };
  }

  // Deep Web & Flutter Root-Cause Debugger (main document + same-origin iframes)
  function auditWebDebug() {
    ensureFlutterSemantics();
    var docs = collectAccessibleDocuments();
    var overlays = [];
    var vw = window.innerWidth || document.documentElement.clientWidth || 1280;
    var docScrollW = document.documentElement
      ? document.documentElement.scrollWidth
      : vw;
    var overflowCulprits = [];
    var contrastIssues = [];

    for (var d = 0; d < docs.length; d++) {
      var frame = docs[d];
      var fdoc = frame.doc;
      var fwin = frame.win || window;

      // 1. Vite Shadow DOM error overlay (<vite-error-overlay>)
      var viteOverlay = fdoc.querySelector("vite-error-overlay");
      if (viteOverlay) {
        var sr = viteOverlay.shadowRoot;
        var msg = sr
          ? String(sr.textContent || "").trim()
          : String(viteOverlay.textContent || "").trim();
        if (msg) {
          overlays.push({ framework: "vite", error: msg.slice(0, 1200) });
        }
      }

      // 2. Next.js portal / error dialog
      var nextPortal = fdoc.querySelector("nextjs-portal");
      if (nextPortal && nextPortal.shadowRoot) {
        var nmsg = String(nextPortal.shadowRoot.textContent || "").trim();
        if (nmsg) {
          overlays.push({ framework: "nextjs", error: nmsg.slice(0, 1200) });
        }
      }

      // 3. Root-cause horizontal overflow & computed CSS culprits
      var fvw =
        d === 0
          ? vw
          : fwin.innerWidth ||
            (fdoc.documentElement && fdoc.documentElement.clientWidth) ||
            vw;
      var allEls = fdoc.querySelectorAll("body *");
      for (var i = 0; i < allEls.length && overflowCulprits.length < 12; i++) {
        var el = allEls[i];
        var rect = el.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) continue;
        if (rect.right > fvw + 2 || rect.left < -2) {
          var cs = fwin.getComputedStyle(el);
          overflowCulprits.push({
            selector: shortSelector(el),
            rect_right: Math.round(rect.right),
            viewport_width: fvw,
            overflow_px: Math.round(rect.right - fvw),
            in_iframe: d > 0,
            computed: {
              display: cs.display,
              position: cs.position,
              width: cs.width,
              minWidth: cs.minWidth,
              whiteSpace: cs.whiteSpace,
              overflowX: cs.overflowX,
              flexShrink: cs.flexShrink
            }
          });
        }
      }

      // 4. WCAG Contrast audit on primary headings, buttons, and links
      var textTargets = fdoc.querySelectorAll("h1, h2, h3, button, a");
      for (var t = 0; t < textTargets.length && contrastIssues.length < 10; t++) {
        var tel = textTargets[t];
        var txt = String(tel.innerText || "").trim();
        if (!txt) continue;
        var tr = tel.getBoundingClientRect();
        if (tr.width < 8 || tr.height < 8) continue;
        var tcs = fwin.getComputedStyle(tel);
        if (tcs.visibility === "hidden" || parseFloat(tcs.opacity || "1") < 0.2)
          continue;
        var fg = parseRgbComponents(tcs.color);
        var bg = resolveEffectiveBg(tel);
        if (fg && bg) {
          var ratio = wcagContrastRatio(fg, bg);
          var minRatio =
            tel.tagName === "H1" || tel.tagName === "H2" ? 3.0 : 4.2;
          if (ratio < minRatio) {
            contrastIssues.push({
              selector: shortSelector(tel),
              text: txt.slice(0, 48),
              contrast_ratio: Number(ratio.toFixed(2)),
              required_ratio: minRatio,
              color: tcs.color,
              in_iframe: d > 0
            });
          }
        }
      }
    }

    // 5. Fixed navbar occlusion check on H1
    var occlusion = null;
    var h1 = document.querySelector("h1");
    var nav = document.querySelector("header, nav");
    if (h1 && nav && !nav.contains(h1)) {
      var navCs = window.getComputedStyle(nav);
      if (navCs.position === "fixed" || navCs.position === "sticky") {
        var nr = nav.getBoundingClientRect();
        var hr = h1.getBoundingClientRect();
        if (hr.top < nr.bottom && hr.bottom > nr.top) {
          occlusion = {
            header_selector: shortSelector(nav),
            header_bottom_px: Math.round(nr.bottom),
            h1_top_px: Math.round(hr.top),
            overlap_px: Math.round(nr.bottom - hr.top)
          };
        }
      }
    }

    return {
      url: location.href,
      title: document.title || "",
      viewport: { width: vw, height: window.innerHeight || 800 },
      page_horizontal_overflow_px: Math.max(0, docScrollW - vw),
      framework_overlays: overlays,
      overflow_culprits: overflowCulprits,
      contrast_issues: contrastIssues,
      fixed_header_occlusion: occlusion,
      failed_network: state.networkLog.filter(function (n) {
        return !n.ok || n.status >= 400;
      }),
      recent_errors: state.consoleLog.filter(function (c) {
        return c.level === "error" || c.level === "warn";
      })
    };
  }

  // Differential UI Tree & Flutter Semantics Snapshot (`computeUiDiff`, main document + same-origin iframes)
  function captureUiDiff(query, maxChars) {
    var fltActive = ensureFlutterSemantics();
    var q = String(query || "").trim().toLowerCase();
    var docs = collectAccessibleDocuments();
    var allItems = [];
    var matchedItems = [];
    var vw = window.innerWidth || 1280;

    for (var d = 0; d < docs.length && allItems.length < 200; d++) {
      var frame = docs[d];
      var nodes = frame.doc.querySelectorAll(
        "flt-semantics[role], a[href], button, input, textarea, select, [role='button'], [role='link'], [role='tab'], [role='menuitem'], [onclick]"
      );
      for (var i = 0; i < nodes.length && allItems.length < 200; i++) {
        var el = nodes[i];
        var r = el.getBoundingClientRect();
        if (r.width < 2 || r.height < 2) continue;
        var tag = el.tagName.toLowerCase();
        var role = el.getAttribute("role") || tag;
        var label = collapseWhitespace(
          el.getAttribute("aria-label") ||
            el.innerText ||
            el.getAttribute("placeholder") ||
            el.getAttribute("title") ||
            ""
        ).slice(0, 90);
        var val =
          typeof el.value === "string" ? el.value.slice(0, 60) : "";
        var idAttr = (el.getAttribute("id") || "").toLowerCase();
        var absLeft = frame.offsetX + r.left;
        var absTop = frame.offsetY + r.top;
        var item = {
          index: allItems.length,
          tag: tag,
          role: role,
          label: label,
          value: val,
          x: Math.round(absLeft + r.width / 2),
          y: Math.round(absTop + r.height / 2),
          w: Math.round(r.width),
          h: Math.round(r.height),
          overflow: absLeft + r.width > vw + 2
        };
        allItems.push(item);
        if (
          !q ||
          label.toLowerCase().indexOf(q) !== -1 ||
          role.toLowerCase().indexOf(q) !== -1 ||
          tag.indexOf(q) !== -1 ||
          idAttr.indexOf(q) !== -1
        ) {
          matchedItems.push(item);
        }
      }
    }

    var prev = state.prevSnapshot;
    state.prevSnapshot = allItems;
    var added = 0;
    var changed = 0;
    if (prev) {
      var prevKeys = {};
      for (var p = 0; p < prev.length; p++) {
        prevKeys[prev[p].role + "|" + prev[p].label] = prev[p];
      }
      for (var c = 0; c < allItems.length; c++) {
        var old = prevKeys[allItems[c].role + "|" + allItems[c].label];
        if (!old) added++;
        else if (old.value !== allItems[c].value || old.overflow !== allItems[c].overflow)
          changed++;
      }
    } else {
      added = allItems.length;
    }

    var limit = maxChars ? Math.max(10, Math.floor(maxChars / 60)) : 120;
    var effective = q && matchedItems.length > 0 ? matchedItems : allItems;

    return {
      flutter_semantics_active: fltActive,
      viewport: { width: vw, height: window.innerHeight || 800 },
      total_elements: effective.length,
      unfiltered_total: allItems.length,
      query_matched: q ? matchedItems.length : allItems.length,
      diff: { has_baseline: Boolean(prev), added: added, changed: changed },
      elements: effective.slice(0, limit)
    };
  }

  var lastPtrX = 640;
  var lastPtrY = 400;
  var lastHoverEl = null;

  function resolvePointDeep(x, y) {
    var win = window;
    var doc = document;
    var localX = x;
    var localY = y;
    var target = doc.elementFromPoint(localX, localY) || doc.body;
    for (var depth = 0; depth < 3 && target && target.tagName === "IFRAME"; depth++) {
      try {
        var childDoc = target.contentDocument;
        var childWin = target.contentWindow;
        if (!childDoc || !childWin) break;
        var r = target.getBoundingClientRect();
        localX = localX - r.left;
        localY = localY - r.top;
        var inner = childDoc.elementFromPoint(localX, localY);
        if (!inner) break;
        doc = childDoc;
        win = childWin;
        target = inner;
      } catch (_) {
        break;
      }
    }
    return { target: target, win: win, x: localX, y: localY };
  }

  function emitPointerAndMouse(target, win, ptrType, mouseType, cx, cy, buttonsMask) {
    if (typeof PointerEvent === "function" && ptrType) {
      try {
        target.dispatchEvent(
          new PointerEvent(ptrType, {
            bubbles: true,
            cancelable: true,
            pointerId: 1,
            pointerType: "mouse",
            isPrimary: true,
            clientX: cx,
            clientY: cy,
            button: mouseType === "mousemove" ? -1 : 0,
            buttons: buttonsMask
          })
        );
      } catch (_) {}
    }
    if (mouseType) {
      target.dispatchEvent(
        new MouseEvent(mouseType, {
          bubbles: true,
          cancelable: true,
          view: win,
          clientX: cx,
          clientY: cy,
          button: 0,
          buttons: buttonsMask
        })
      );
    }
  }

  // agweb Motion.swift + browser-harness click_at_xy inspired pointer dispatcher:
  // Interpolates a 5-step cubic-Bezier path from the previous pointer coordinate and
  // pierces same-origin iframes so Radix/Floating-UI hover menus and drag sensors fire reliably.
  function dispatchSyntheticPointer(type, x, y, deltaX, deltaY) {
    var resolved = resolvePointDeep(x, y);
    var target = resolved.target;
    if (!target) return false;
    if (type === "mouseWheel") {
      (resolved.win || window).scrollBy(deltaX || 0, deltaY || 0);
      target.dispatchEvent(
        new WheelEvent("wheel", {
          bubbles: true,
          cancelable: true,
          clientX: resolved.x,
          clientY: resolved.y,
          deltaX: deltaX || 0,
          deltaY: deltaY || 0
        })
      );
      return true;
    }

    var sx = lastPtrX;
    var sy = lastPtrY;
    var dx = x - sx;
    var dy = y - sy;
    var dist = Math.sqrt(dx * dx + dy * dy);
    if ((type === "mouseMoved" || type === "mousePressed") && dist > 6) {
      var c1x = sx + dx * 0.3 + dy * 0.08;
      var c1y = sy + dy * 0.3 - dx * 0.08;
      var c2x = sx + dx * 0.7 - dy * 0.05;
      var c2y = sy + dy * 0.7 + dx * 0.05;
      for (var step = 1; step <= 4; step++) {
        var t = step / 5;
        var inv = 1 - t;
        var ix =
          inv * inv * inv * sx +
          3 * inv * inv * t * c1x +
          3 * inv * t * t * c2x +
          t * t * t * x;
        var iy =
          inv * inv * inv * sy +
          3 * inv * inv * t * c1y +
          3 * inv * t * t * c2y +
          t * t * t * y;
        var mid = resolvePointDeep(ix, iy);
        if (mid.target) {
          emitPointerAndMouse(mid.target, mid.win, "pointermove", "mousemove", mid.x, mid.y, 0);
        }
      }
    }
    lastPtrX = x;
    lastPtrY = y;

    if (target !== lastHoverEl) {
      if (lastHoverEl && lastHoverEl.isConnected) {
        emitPointerAndMouse(lastHoverEl, window, "pointerout", "mouseout", resolved.x, resolved.y, 0);
      }
      emitPointerAndMouse(target, resolved.win, "pointerover", "mouseover", resolved.x, resolved.y, 0);
      lastHoverEl = target;
    }

    if (type === "mouseMoved") {
      emitPointerAndMouse(target, resolved.win, "pointermove", "mousemove", resolved.x, resolved.y, 0);
    } else if (type === "mousePressed") {
      try {
        if (typeof target.focus === "function") target.focus();
      } catch (_) {}
      emitPointerAndMouse(target, resolved.win, "pointerdown", "mousedown", resolved.x, resolved.y, 1);
    } else if (type === "mouseReleased") {
      emitPointerAndMouse(target, resolved.win, "pointerup", "mouseup", resolved.x, resolved.y, 0);
      target.dispatchEvent(
        new MouseEvent("click", {
          bubbles: true,
          cancelable: true,
          view: resolved.win,
          clientX: resolved.x,
          clientY: resolved.y,
          button: 0
        })
      );
    }
    return true;
  }

  var SENSITIVE_STORAGE_TOKENS = [
    "token",
    "secret",
    "password",
    "passwd",
    "apikey",
    "api_key",
    "api-key",
    "auth",
    "session",
    "cookie",
    "jwt",
    "private"
  ];

  function isSensitiveStorageKey(key) {
    var k = String(key || "").toLowerCase();
    for (var i = 0; i < SENSITIVE_STORAGE_TOKENS.length; i++) {
      if (k.indexOf(SENSITIVE_STORAGE_TOKENS[i]) !== -1) return true;
    }
    return false;
  }

  function readSafeStorage(storeObj) {
    var out = {};
    if (!storeObj) return out;
    try {
      for (var i = 0; i < storeObj.length && i < 60; i++) {
        var k = storeObj.key(i);
        if (!k) continue;
        var raw = String(storeObj.getItem(k) || "");
        out[k] = isSensitiveStorageKey(k) ? "[MASKED]" : raw.slice(0, 160);
      }
    } catch (_) {}
    return out;
  }

  function buildNetworkWaterfall() {
    var navTiming = null;
    var slowResources = [];
    try {
      if (window.performance && window.performance.getEntriesByType) {
        var navs = window.performance.getEntriesByType("navigation");
        if (navs && navs.length > 0) {
          var n = navs[0];
          navTiming = {
            ttfb_ms: Math.round(Math.max(0, (n.responseStart || 0) - (n.requestStart || 0))),
            dom_interactive_ms: Math.round(n.domInteractive || 0),
            dom_content_loaded_ms: Math.round(n.domContentLoadedEventEnd || 0),
            load_ms: Math.round(n.loadEventEnd || 0)
          };
        }
        var resList = window.performance.getEntriesByType("resource") || [];
        var sorted = Array.prototype.slice.call(resList).sort(function (a, b) {
          return (b.duration || 0) - (a.duration || 0);
        });
        for (var r = 0; r < sorted.length && slowResources.length < 15; r++) {
          slowResources.push({
            name: String(sorted[r].name || "").slice(0, 140),
            initiator: sorted[r].initiatorType || "other",
            duration_ms: Math.round(sorted[r].duration || 0),
            transfer_bytes: sorted[r].transferSize || 0
          });
        }
      }
    } catch (_) {}

    return {
      url: location.href,
      navigation_timing: navTiming,
      total_fetch_requests: state.networkLog.length,
      failed_count: state.networkLog.filter(function (e) {
        return !e.ok || e.status >= 400;
      }).length,
      requests: state.networkLog.slice(-60),
      slowest_resources: slowResources
    };
  }

  function inspectStorageState() {
    var cookieNames = [];
    try {
      var parts = String(document.cookie || "").split(";");
      for (var i = 0; i < parts.length; i++) {
        var eq = parts[i].indexOf("=");
        var cname = (eq >= 0 ? parts[i].slice(0, eq) : parts[i]).trim();
        if (cname) cookieNames.push(cname);
      }
    } catch (_) {}

    return {
      url: location.href,
      origin: location.origin || "",
      local_storage: readSafeStorage(window.localStorage),
      session_storage: readSafeStorage(window.sessionStorage),
      cookie_names: cookieNames
    };
  }

  function dispatchSyntheticKeyboard(type, key, code, modifiers) {
    var target = document.activeElement || document.body;
    if (!target) return false;
    var mods = modifiers || 0;
    var evInit = {
      key: key || "",
      code: code || key || "",
      bubbles: true,
      cancelable: true,
      altKey: Boolean(mods & 1),
      ctrlKey: Boolean(mods & 2),
      metaKey: Boolean(mods & 4),
      shiftKey: Boolean(mods & 8)
    };
    var domMap = {
      keyDown: "keydown",
      rawKeyDown: "keydown",
      keyUp: "keyup",
      char: "keypress"
    };
    var domType = domMap[type] || "keydown";
    var notCancelled = target.dispatchEvent(new KeyboardEvent(domType, evInit));

    if (domType === "keydown" && notCancelled) {
      if (key === "Enter" && target.form) {
        try {
          if (typeof target.form.requestSubmit === "function") {
            target.form.requestSubmit();
          } else {
            target.form.submit();
          }
        } catch (_) {}
      } else if (key === "Escape") {
        var openDialog = target.closest ? target.closest("dialog[open]") : null;
        if (openDialog && typeof openDialog.close === "function") {
          try {
            openDialog.close();
          } catch (_) {}
        }
      } else if (key === "Tab") {
        var focusables = Array.prototype.slice.call(
          document.querySelectorAll(
            "a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])"
          )
        );
        var curIdx = focusables.indexOf(target);
        if (focusables.length > 0) {
          var step = evInit.shiftKey ? -1 : 1;
          var nextIdx =
            (curIdx + step + focusables.length) % focusables.length;
          if (focusables[nextIdx] && typeof focusables[nextIdx].focus === "function") {
            focusables[nextIdx].focus();
          }
        }
      }
    }
    return true;
  }

  function setFileInputFiles(selector, fileSpecs) {
    var docs = collectAccessibleDocuments();
    var inputEl = null;
    for (var d = 0; d < docs.length && !inputEl; d++) {
      try {
        inputEl = selector
          ? docs[d].doc.querySelector(selector)
          : docs[d].doc.querySelector("input[type='file']");
      } catch (_) {}
    }
    if (!inputEl || typeof DataTransfer !== "function" || typeof File !== "function") {
      return { ok: false, error: "File input target or DataTransfer unavailable" };
    }
    var dt = new DataTransfer();
    var specs = Array.isArray(fileSpecs) ? fileSpecs : [];
    for (var i = 0; i < specs.length; i++) {
      var sp = specs[i];
      var binStr = atob(String(sp.base64 || ""));
      var bytes = new Uint8Array(binStr.length);
      for (var b = 0; b < binStr.length; b++) {
        bytes[b] = binStr.charCodeAt(b);
      }
      dt.items.add(
        new File([bytes], String(sp.name || "upload.bin"), {
          type: String(sp.mime || "application/octet-stream")
        })
      );
    }
    inputEl.files = dt.files;
    inputEl.dispatchEvent(new Event("input", { bubbles: true }));
    inputEl.dispatchEvent(new Event("change", { bubbles: true }));
    return { ok: true, count: dt.files.length };
  }

  function collectInteractiveBoxes(maxCount) {
    var limit = typeof maxCount === "number" && maxCount > 0 ? maxCount : 50;
    var docs = collectAccessibleDocuments();
    var out = [];
    for (var d = 0; d < docs.length && out.length < limit; d++) {
      var frame = docs[d];
      var nodes = frame.doc.querySelectorAll
        ? frame.doc.querySelectorAll(
            "button, a[href], input, select, textarea, flt-semantics, [role='button'], [role='link'], [role='tab'], [onclick]"
          )
        : [];
      for (var i = 0; i < nodes.length && out.length < limit; i++) {
        var el = nodes[i];
        var rect = el.getBoundingClientRect();
        if (rect.width < 4 || rect.height < 4) continue;
        try {
          var cs = (frame.win || window).getComputedStyle(el);
          if (cs.display === "none" || cs.visibility === "hidden" || cs.opacity === "0") continue;
        } catch (_) {}
        var refAttr = el.getAttribute("data-minicode-ref") || "";
        var label = refAttr
          ? refAttr.replace("@", "")
          : "e" + String(out.length + 1);
        out.push({
          label: label,
          x: Math.round(rect.left + frame.offsetX),
          y: Math.round(rect.top + frame.offsetY),
          w: Math.round(rect.width),
          h: Math.round(rect.height)
        });
      }
    }
    return out;
  }

  function getApiTraffic(filterPattern, onlyErrors) {
    var items = [];
    for (var i = 0; i < state.apiTraffic.length; i++) {
      var entry = state.apiTraffic[i];
      if (onlyErrors && entry.ok) continue;
      if (filterPattern && !matchesUrlPattern(filterPattern, entry.url)) continue;
      items.push(entry);
    }
    return {
      url: location.href,
      total_captured: state.apiTraffic.length,
      matched: items.length,
      entries: items
    };
  }

  // ─────────────────────────────────────────────────────────────────────────
  //  ego-lite + browser-act Inspired In-Page Agent SDK (`window.mb` / `window.ego`)
  //  Enables single-pass code-driven multi-step automation & table extraction
  //  with zero regex and shorthand ref resolution (`1`, `"@1"`, `"e1"`, `"@v1:e1"`).
  // ─────────────────────────────────────────────────────────────────────────
  var sdkRefMap = {};

  function normalizeSdkRefKey(raw) {
    if (typeof raw === "number" && raw > 0) return "e" + String(Math.floor(raw));
    var s = String(raw || "").trim();
    if (!s) return "";
    if (s.charAt(0) === "[" && s.charAt(s.length - 1) === "]") {
      s = s.slice(1, -1).trim();
    }
    if (s.indexOf("@v") === 0) {
      var colon = s.indexOf(":");
      if (colon > 2) s = s.slice(colon + 1);
    } else if (s.charAt(0) === "@") {
      s = s.slice(1);
    }
    var allDigits = s.length > 0;
    for (var i = 0; i < s.length; i++) {
      var ch = s.charCodeAt(i);
      if (ch < 48 || ch > 57) {
        allDigits = false;
        break;
      }
    }
    if (allDigits) return "e" + s;
    if (s.charAt(0) === "e" || s.charAt(0) === "E") {
      var rest = s.slice(1);
      var restDigits = rest.length > 0;
      for (var j = 0; j < rest.length; j++) {
        var rc = rest.charCodeAt(j);
        if (rc < 48 || rc > 57) {
          restDigits = false;
          break;
        }
      }
      if (restDigits) return "e" + rest;
    }
    return "";
  }

  function sdkSnapshot(maxElements) {
    var limit = typeof maxElements === "number" && maxElements > 0 ? maxElements : 60;
    var docs = collectAccessibleDocuments();
    sdkRefMap = {};
    var items = [];
    for (var d = 0; d < docs.length && items.length < limit; d++) {
      var frame = docs[d];
      var nodes = frame.doc.querySelectorAll
        ? frame.doc.querySelectorAll(
            "button, a[href], input, select, textarea, summary, flt-semantics[role], [role='button'], [role='link'], [role='tab'], [role='menuitem'], [role='checkbox'], [onclick]"
          )
        : [];
      for (var i = 0; i < nodes.length && items.length < limit; i++) {
        var el = nodes[i];
        var rect = el.getBoundingClientRect();
        if (rect.width < 3 || rect.height < 3) continue;
        try {
          var cs = (frame.win || window).getComputedStyle(el);
          if (cs.display === "none" || cs.visibility === "hidden" || cs.opacity === "0") continue;
        } catch (_) {}
        var idx = items.length + 1;
        var refKey = "e" + String(idx);
        var existingAttr = el.getAttribute("data-minicode-ref") || "";
        if (!existingAttr) {
          try {
            el.setAttribute("data-minicode-ref", "@" + refKey);
          } catch (_) {}
        }
        sdkRefMap[refKey] = el;
        var tag = el.tagName.toLowerCase();
        var role = el.getAttribute("role") || tag;
        var inputType = (el.getAttribute("type") || "").toLowerCase();
        var isSensitive =
          inputType === "password" ||
          isSensitiveStorageKey(el.getAttribute("name") || el.getAttribute("id") || "");
        var label = collapseWhitespace(
          el.getAttribute("aria-label") ||
            el.innerText ||
            el.getAttribute("placeholder") ||
            el.getAttribute("title") ||
            el.getAttribute("name") ||
            ""
        ).slice(0, 90);
        var val = "";
        if (typeof el.value === "string" && el.value.length > 0) {
          val = isSensitive ? "[REDACTED]" : el.value.slice(0, 60);
        }
        items.push({
          index: idx,
          ref: refKey,
          tag: tag,
          role: role,
          name: label,
          value: val,
          selector: shortSelector(el)
        });
      }
    }
    return {
      url: location.href,
      title: document.title || "",
      count: items.length,
      elements: items
    };
  }

  function resolveSdkElement(refOrSelector) {
    if (!refOrSelector && refOrSelector !== 0) return null;
    if (refOrSelector && typeof refOrSelector === "object" && refOrSelector.nodeType === 1) {
      return refOrSelector;
    }
    var refKey = normalizeSdkRefKey(refOrSelector);
    if (refKey) {
      if (!sdkRefMap[refKey] || !sdkRefMap[refKey].isConnected) {
        sdkSnapshot(80);
      }
      if (sdkRefMap[refKey] && sdkRefMap[refKey].isConnected) {
        return sdkRefMap[refKey];
      }
      var docs = collectAccessibleDocuments();
      for (var d = 0; d < docs.length; d++) {
        var byRef = docs[d].doc.querySelector(
          "[data-minicode-ref$=':" + refKey + "'], [data-minicode-ref='@" + refKey + "']"
        );
        if (byRef) return byRef;
      }
    }
    var sel = String(refOrSelector || "").trim();
    if (!sel) return null;
    var docs2 = collectAccessibleDocuments();
    for (var f = 0; f < docs2.length; f++) {
      try {
        var found = docs2[f].doc.querySelector(sel);
        if (found) return found;
      } catch (_) {}
    }
    return null;
  }

  function sdkClick(refOrSelector) {
    var el = resolveSdkElement(refOrSelector);
    if (!el) return { ok: false, error: "Element not found: " + String(refOrSelector) };
    try {
      if (el.scrollIntoView) el.scrollIntoView({ block: "center", inline: "center" });
    } catch (_) {}
    var r = el.getBoundingClientRect();
    var cx = Math.round(r.left + r.width / 2);
    var cy = Math.round(r.top + r.height / 2);
    try {
      if (typeof el.focus === "function") el.focus();
      el.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true, clientX: cx, clientY: cy }));
      el.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, cancelable: true, clientX: cx, clientY: cy }));
      if (typeof el.click === "function") el.click();
      else el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, clientX: cx, clientY: cy }));
    } catch (err) {
      return { ok: false, error: String(err) };
    }
    return {
      ok: true,
      tag: el.tagName.toLowerCase(),
      selector: shortSelector(el),
      url: location.href
    };
  }

  function sdkFill(refOrSelector, text) {
    var el = resolveSdkElement(refOrSelector);
    if (!el) return { ok: false, error: "Input element not found: " + String(refOrSelector) };
    var strVal = String(text == null ? "" : text);
    try {
      if (typeof el.focus === "function") el.focus();
      var proto =
        el.tagName === "TEXTAREA"
          ? window.HTMLTextAreaElement && window.HTMLTextAreaElement.prototype
          : window.HTMLInputElement && window.HTMLInputElement.prototype;
      var desc = proto ? Object.getOwnPropertyDescriptor(proto, "value") : null;
      if (desc && typeof desc.set === "function") {
        desc.set.call(el, strVal);
      } else if ("value" in el) {
        el.value = strVal;
      } else if (el.isContentEditable) {
        el.textContent = strVal;
      }
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
    } catch (err) {
      return { ok: false, error: String(err) };
    }
    return { ok: true, selector: shortSelector(el) };
  }

  function extractStructuredTables(selector) {
    var docs = collectAccessibleDocuments();
    var tablesOut = [];
    var querySel = selector ? String(selector) : "table, [role='table'], [role='grid']";
    for (var d = 0; d < docs.length && tablesOut.length < 15; d++) {
      var nodes = [];
      try {
        nodes = docs[d].doc.querySelectorAll(querySel);
      } catch (_) {}
      for (var t = 0; t < nodes.length && tablesOut.length < 15; t++) {
        var tbl = nodes[t];
        var headers = [];
        var ths = tbl.querySelectorAll("thead th, thead td, [role='columnheader']");
        if (!ths || ths.length === 0) {
          var firstRow = tbl.querySelector("tr, [role='row']");
          if (firstRow) ths = firstRow.querySelectorAll("th, [role='columnheader']");
        }
        for (var h = 0; h < ths.length; h++) {
          var hText = collapseWhitespace(ths[h].innerText || ths[h].textContent || "");
          headers.push(hText || "col_" + String(h + 1));
        }
        var trs = tbl.querySelectorAll("tbody tr, tr, [role='row']");
        var rows = [];
        var seenRowEls = [];
        for (var r = 0; r < trs.length && rows.length < 200; r++) {
          var tr = trs[r];
          if (seenRowEls.indexOf(tr) !== -1) continue;
          seenRowEls.push(tr);
          var cells = tr.querySelectorAll("td, [role='cell'], [role='gridcell']");
          if (!cells || cells.length === 0) continue;
          if (headers.length > 0) {
            var rowObj = {};
            for (var c = 0; c < cells.length; c++) {
              var key = headers[c] || "col_" + String(c + 1);
              rowObj[key] = collapseWhitespace(cells[c].innerText || cells[c].textContent || "");
            }
            rows.push(rowObj);
          } else {
            var rowArr = [];
            for (var c2 = 0; c2 < cells.length; c2++) {
              rowArr.push(collapseWhitespace(cells[c2].innerText || cells[c2].textContent || ""));
            }
            rows.push(rowArr);
          }
        }
        if (headers.length > 0 || rows.length > 0) {
          tablesOut.push({
            index: tablesOut.length,
            selector: shortSelector(tbl),
            headers: headers,
            row_count: rows.length,
            rows: rows
          });
        }
      }
    }
    return {
      url: location.href,
      title: document.title || "",
      table_count: tablesOut.length,
      tables: tablesOut
    };
  }

  function sdkExtractLinks(filterSubstr) {
    var needle = String(filterSubstr || "").trim().toLowerCase();
    var docs = collectAccessibleDocuments();
    var seen = {};
    var links = [];
    for (var d = 0; d < docs.length && links.length < 150; d++) {
      var anchors = docs[d].doc.querySelectorAll("a[href]");
      for (var i = 0; i < anchors.length && links.length < 150; i++) {
        var a = anchors[i];
        var href = String(a.href || a.getAttribute("href") || "").trim();
        if (!href || href.indexOf("javascript:") === 0 || seen[href]) continue;
        var text = collapseWhitespace(a.innerText || a.getAttribute("aria-label") || "");
        if (
          needle &&
          href.toLowerCase().indexOf(needle) === -1 &&
          text.toLowerCase().indexOf(needle) === -1
        ) {
          continue;
        }
        seen[href] = true;
        links.push({ text: text.slice(0, 100), href: href });
      }
    }
    return links;
  }

  function sdkPaginate(opts) {
    var cfg = opts || {};
    var nextTarget = cfg.next || cfg.nextRef || cfg.nextSelector || "a[rel='next'], button[aria-label*='Next'], [aria-label*='next']";
    var maxPages = Math.min(Math.max(Number(cfg.maxPages || cfg.limit || 3), 1), 10);
    var itemSel = cfg.itemSelector || null;
    var waitMs = Math.min(Math.max(Number(cfg.waitMs || 250), 20), 3000);
    var pages = [];
    var prevSignature = "";

    function collectCurrentPage(pageNum) {
      var entry = { page: pageNum, url: location.href };
      if (itemSel) {
        var nodes = document.querySelectorAll(itemSel);
        var items = [];
        for (var i = 0; i < nodes.length && items.length < 100; i++) {
          items.push(collapseWhitespace(nodes[i].innerText || nodes[i].textContent || ""));
        }
        entry.items = items;
        entry.sig = location.href + "|" + items.join("||").slice(0, 400);
      } else {
        var t = extractStructuredTables(cfg.tableSelector);
        entry.tables = t.tables;
        entry.sig = location.href + "|" + JSON.stringify(t.tables).slice(0, 400);
      }
      return entry;
    }

    function step(pageNum) {
      return waitForIdle(80, waitMs).then(function () {
        var cur = collectCurrentPage(pageNum);
        if (cur.sig === prevSignature) {
          return { stopped_reason: "duplicate_page", pages_collected: pages.length, pages: pages };
        }
        prevSignature = cur.sig;
        delete cur.sig;
        pages.push(cur);
        if (pageNum >= maxPages) {
          return { stopped_reason: "max_pages_reached", pages_collected: pages.length, pages: pages };
        }
        var el = resolveSdkElement(nextTarget);
        if (!el || el.disabled || el.getAttribute("aria-disabled") === "true") {
          return { stopped_reason: "no_next_control", pages_collected: pages.length, pages: pages };
        }
        var clicked = sdkClick(el);
        if (!clicked.ok) {
          return { stopped_reason: "click_failed", pages_collected: pages.length, pages: pages };
        }
        return new Promise(function (resolve) {
          setTimeout(function () {
            resolve(step(pageNum + 1));
          }, waitMs);
        });
      });
    }

    return step(1);
  }

  var agentSdk = {
    snapshot: sdkSnapshot,
    click: sdkClick,
    fill: sdkFill,
    waitFor: function (selectorOrText, timeoutMs) {
      var s = String(selectorOrText || "").trim();
      var ms = typeof timeoutMs === "number" ? timeoutMs : 4000;
      if (s.indexOf("text:") === 0) {
        var needle = s.slice(5).trim().toLowerCase();
        var started = Date.now();
        return new Promise(function (resolve) {
          function checkText() {
            var bodyTxt = String((document.body && document.body.innerText) || "").toLowerCase();
            if (bodyTxt.indexOf(needle) !== -1) {
              resolve({ ok: true, matched_text: needle, elapsed_ms: Date.now() - started });
              return true;
            }
            return false;
          }
          if (checkText()) return;
          var obs = new MutationObserver(function () {
            if (checkText()) {
              obs.disconnect();
              clearTimeout(tmr);
            }
          });
          obs.observe(document.documentElement || document, {
            childList: true,
            subtree: true,
            characterData: true
          });
          var tmr = setTimeout(function () {
            obs.disconnect();
            resolve({ ok: false, timeout: true, matched_text: needle, elapsed_ms: Date.now() - started });
          }, ms);
        });
      }
      return waitForSelector(s, "visible", ms);
    },
    extractTables: extractStructuredTables,
    extractLinks: sdkExtractLinks,
    paginate: sdkPaginate
  };

  // Smart Full-Browser Media & Memory Diet:
  // Keeps 100% playback compatibility (YouTube, Vimeo, HTML5 MSE, WebAudio) while steering
  // adaptive streaming players away from CPU/RAM-heavy software VP9/AV1/4K60 into crisp H.264 (avc1 <= 720p30),
  // pruning YouTube's invisible `#cinematics` ambient-glow canvas, and pausing video paint when minimized.
  var mediaDietState = {
    enabled: true,
    audioOnly: false,
    minimized: false
  };

  function isHeavySoftwareCodecQuery(mimeStr) {
    if (!mediaDietState.enabled || !mimeStr) return false;
    var s = String(mimeStr).toLowerCase();
    if (
      s.indexOf("vp09") !== -1 ||
      s.indexOf("vp9") !== -1 ||
      s.indexOf("av01") !== -1 ||
      s.indexOf("av1") !== -1 ||
      s.indexOf("hev1") !== -1 ||
      s.indexOf("hvc1") !== -1 ||
      s.indexOf("smpte2084") !== -1 ||
      s.indexOf("framerate=60") !== -1
    ) {
      return true;
    }
    var wIdx = s.indexOf("width=");
    if (wIdx !== -1) {
      var wVal = parseInt(s.slice(wIdx + 6), 10);
      if (!isNaN(wVal) && wVal > 1280) return true;
    }
    var hIdx = s.indexOf("height=");
    if (hIdx !== -1) {
      var hVal = parseInt(s.slice(hIdx + 7), 10);
      if (!isNaN(hVal) && hVal > 720) return true;
    }
    return false;
  }

  (function installSmartMediaDiet() {
    try {
      if (
        typeof MediaSource !== "undefined" &&
        typeof MediaSource.isTypeSupported === "function"
      ) {
        var origMseSupport = MediaSource.isTypeSupported.bind(MediaSource);
        MediaSource.isTypeSupported = function (type) {
          if (isHeavySoftwareCodecQuery(type)) return false;
          return origMseSupport(type);
        };
      }
      if (
        typeof HTMLMediaElement !== "undefined" &&
        HTMLMediaElement.prototype &&
        typeof HTMLMediaElement.prototype.canPlayType === "function"
      ) {
        var origCanPlay = HTMLMediaElement.prototype.canPlayType;
        HTMLMediaElement.prototype.canPlayType = function (type) {
          if (isHeavySoftwareCodecQuery(type)) return "";
          return origCanPlay.call(this, type);
        };
      }
    } catch (_) {}
  })();

  function syncVideoPaintSaverStyle() {
    try {
      if (!document || !document.head) return;
      var styleId = "__minibrowse_video_paint_saver";
      var existing = document.getElementById(styleId);
      var suppressVideoPaint = mediaDietState.minimized || mediaDietState.audioOnly;
      if (suppressVideoPaint) {
        if (!existing) {
          existing = document.createElement("style");
          existing.id = styleId;
          existing.setAttribute("data-minicode-ignore", "true");
          existing.textContent =
            "video, #cinematics, ytd-player #cinematics canvas { opacity: 0 !important; visibility: hidden !important; filter: none !important; }";
          document.head.appendChild(existing);
        }
      } else if (existing && existing.parentNode) {
        existing.parentNode.removeChild(existing);
      }
    } catch (_) {}
  }

  function isPlayingMedia() {
    try {
      if (!document) return false;
      var mediaEls = document.querySelectorAll("video, audio");
      for (var i = 0; i < mediaEls.length; i++) {
        var m = mediaEls[i];
        if (!m.paused && !m.ended && m.readyState >= 2) {
          return true;
        }
      }
    } catch (_) {}
    return false;
  }

  function trimMediaAndCaches() {
    var removedCinematics = 0;
    try {
      if (document && mediaDietState.enabled) {
        var ambients = document.querySelectorAll(
          "#cinematics canvas, ytd-watch-flexy #cinematics"
        );
        for (var i = 0; i < ambients.length; i++) {
          var node = ambients[i];
          if (node && node.style) {
            node.style.display = "none";
            removedCinematics++;
          }
        }
      }
    } catch (_) {}
    syncVideoPaintSaverStyle();
    return {
      playing: isPlayingMedia(),
      minimized: mediaDietState.minimized,
      audio_only: mediaDietState.audioOnly,
      suppressed_ambient_nodes: removedCinematics
    };
  }

  window.mb = agentSdk;
  if (!window.ego) {
    window.ego = agentSdk;
  }

  window.__minibrowse = {
    sdk: agentSdk,
    extractTables: extractStructuredTables,
    isPlayingMedia: isPlayingMedia,
    trimMediaAndCaches: trimMediaAndCaches,
    setWindowMinimized: function (minimized) {
      mediaDietState.minimized = Boolean(minimized);
      syncVideoPaintSaverStyle();
      return mediaDietState.minimized;
    },
    setMediaDiet: function (opts) {
      if (opts && typeof opts === "object") {
        if (typeof opts.enabled === "boolean") mediaDietState.enabled = opts.enabled;
        if (typeof opts.audioOnly === "boolean") mediaDietState.audioOnly = opts.audioOnly;
      }
      return trimMediaAndCaches();
    },
    setMockRoutes: function (routes) {
      state.mockRoutes = Array.isArray(routes) ? routes : [];
      return state.mockRoutes.length;
    },
    setBlockedPatterns: function (patterns) {
      state.blockedPatterns = Array.isArray(patterns) ? patterns : [];
      return state.blockedPatterns.length;
    },
    setAdBlock: function (enabled) {
      state.adBlockEnabled = Boolean(enabled);
      return state.adBlockEnabled;
    },
    configureDialog: function (accept, promptText) {
      state.dialogAccept = accept !== false;
      state.dialogPromptText =
        typeof promptText === "string" ? promptText : null;
      return { accept: state.dialogAccept, dialogs_seen: state.dialogLog.length };
    },
    setFileInputFiles: setFileInputFiles,
    extractResearch: extractResearch,
    extractSemanticTree: extractSemanticTree,
    auditWebDebug: auditWebDebug,
    captureUiDiff: captureUiDiff,
    dispatchPointer: dispatchSyntheticPointer,
    dispatchKeyboard: dispatchSyntheticKeyboard,
    networkWaterfall: buildNetworkWaterfall,
    apiTraffic: getApiTraffic,
    interactiveBoxes: collectInteractiveBoxes,
    inspectStorage: inspectStorageState,
    waitForIdle: waitForIdle,
    waitForSelector: waitForSelector,
    inflightCount: function () {
      return state.inflightRequests;
    },
    dialogLog: function () {
      return state.dialogLog;
    },
    networkLog: function () {
      return state.networkLog;
    },
    clearDiagnostics: function () {
      state.networkLog = [];
      state.apiTraffic = [];
      state.consoleLog = [];
      state.dialogLog = [];
      return true;
    }
  };
})();
