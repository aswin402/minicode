use super::driver::CdpClient;
use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Embedded in-page probe that runs inside the browser DOM context.
/// Pierces Shadow DOM, computes accurate layout geometry, and checks accessibility.
pub const PAGE_PROBE_JS: &str = r###"(function() {
    if (window.__minicode_page_agent) return 'already_installed';

    var SCROLL_MODES = { auto: 1, scroll: 1, overlay: 1 };
    var NATIVE_INTERACTIVE_TAGS = {
        button: 1,
        a: 1,
        input: 1,
        select: 1,
        textarea: 1,
        summary: 1,
        'flt-semantics': 1
    };
    var ARIA_INTERACTIVE_ROLES = {
        button: 1,
        link: 1,
        checkbox: 1,
        radio: 1,
        combobox: 1,
        menuitem: 1,
        tab: 1,
        switch: 1,
        slider: 1,
        option: 1,
        treeitem: 1,
        textbox: 1
    };
    var SENSITIVE_HINTS = [
        'password',
        'passwd',
        'secret',
        'apikey',
        'api_key',
        'api-key',
        'accesstoken',
        'access_token',
        'access-token',
        'cc-number',
        'cc-csc',
        'cvv',
        'ssn'
    ];

    function hasSensitiveHint(haystack) {
        var s = String(haystack || '').toLowerCase();
        for (var i = 0; i < SENSITIVE_HINTS.length; i++) {
            if (s.indexOf(SENSITIVE_HINTS[i]) !== -1) return true;
        }
        return false;
    }

    function collapseSpaces(str) {
        var s = String(str || '');
        var out = [];
        var cur = '';
        for (var i = 0; i < s.length; i++) {
            var code = s.charCodeAt(i);
            if (code === 32 || code === 9 || code === 10 || code === 13 || code === 12) {
                if (cur) {
                    out.push(cur);
                    cur = '';
                }
            } else {
                cur += s.charAt(i);
            }
        }
        if (cur) out.push(cur);
        return out.join(' ');
    }

    window.__minicode_page_agent = {
        // Pierces light DOM, Shadow DOM, and Flutter Semantics to find visible interactive elements and stamps DOM refs
        scanInteractables: function() {
            var fltPlaceholder = document.querySelector('flt-semantics-placeholder');
            if (fltPlaceholder) {
                try { fltPlaceholder.click(); } catch (_) {}
            }
            var results = [];
            var idCounter = 1;
            var rev = window.__minicode_rev || 1;

            function getShortSelector(el) {
                function isUniqueSelector(sel) {
                    try {
                        var matches = document.querySelectorAll(sel);
                        return matches.length === 1 && matches[0] === el;
                    } catch (_) {
                        return false;
                    }
                }

                if (el.id) {
                    var idSel = '#' + CSS.escape(el.id);
                    if (isUniqueSelector(idSel)) return idSel;
                }
                var attrCandidates = ['data-testid', 'name', 'aria-label'];
                var tagLower = el.tagName.toLowerCase();
                for (var aIdx = 0; aIdx < attrCandidates.length; aIdx++) {
                    var attrName = attrCandidates[aIdx];
                    var attrVal = el.getAttribute(attrName);
                    if (attrVal) {
                        var attrSel = (attrName === 'data-testid' ? '' : tagLower) +
                            '[' + attrName + '="' + CSS.escape(attrVal) + '"]';
                        if (isUniqueSelector(attrSel)) return attrSel;
                    }
                }

                var parts = [];
                var cur = el;
                while (cur && cur.nodeType === 1 && cur !== document.documentElement) {
                    if (cur.id && cur !== el) {
                        var ancId = '#' + CSS.escape(cur.id);
                        try {
                            if (document.querySelectorAll(ancId).length === 1) {
                                parts.unshift(ancId);
                                break;
                            }
                        } catch (_) {}
                    }
                    if (cur === document.body) {
                        parts.unshift('body');
                        break;
                    }
                    var segTag = cur.tagName.toLowerCase();
                    var parent = cur.parentElement;
                    if (!parent) {
                        parts.unshift(segTag);
                        break;
                    }
                    var sameTagSiblings = Array.from(parent.children).filter(function(c) {
                        return c.tagName === cur.tagName;
                    });
                    if (sameTagSiblings.length > 1) {
                        parts.unshift(segTag + ':nth-of-type(' + (sameTagSiblings.indexOf(cur) + 1) + ')');
                    } else {
                        parts.unshift(segTag);
                    }
                    cur = parent;
                }
                return parts.join(' > ') || tagLower;
            }

            function isElementVisible(el, rect, style) {
                if (rect.width <= 0 || rect.height <= 0) return false;
                if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
                if (style.pointerEvents === 'none') return false;
                return true;
            }

            function isIgnoredRoot(el) {
                if (el === document.body || el === document.documentElement) return true;
                if (el.matches && el.matches('[data-reactroot], [data-reactid], [data-react-checksum], #root, #app, [id^="root-"], [id^="app-"], #adex-wrapper, #adex-root, #minicode-simulator-container')) {
                    return true;
                }
                return false;
            }

            function checkScrollable(el, style) {
                var hasScrollY = SCROLL_MODES[style.overflowY] === 1 && el.scrollHeight > el.clientHeight;
                var hasScrollX = SCROLL_MODES[style.overflowX] === 1 && el.scrollWidth > el.clientWidth;
                if (hasScrollY || hasScrollX) {
                    return {
                        top: Math.round(el.scrollTop),
                        bottom: Math.round(Math.max(0, el.scrollHeight - el.clientHeight - el.scrollTop)),
                        left: Math.round(el.scrollLeft),
                        right: Math.round(Math.max(0, el.scrollWidth - el.clientWidth - el.scrollLeft))
                    };
                }
                return null;
            }

            function walk(root, inShadow) {
                var elements = root.querySelectorAll ? Array.from(root.querySelectorAll('*')) : [];
                for (var i = 0; i < elements.length; i++) {
                    var el = elements[i];

                    // Pierce Shadow DOM
                    if (el.shadowRoot) {
                        walk(el.shadowRoot, true);
                    }

                    if (isIgnoredRoot(el)) continue;

                    var tag = el.tagName.toLowerCase();
                    var role = (el.getAttribute('role') || tag).toLowerCase();
                    var isNativeInteractive = NATIVE_INTERACTIVE_TAGS[tag] === 1;
                    var isAriaInteractive = ARIA_INTERACTIVE_ROLES[role] === 1;
                    var hasClick = el.onclick !== null || el.hasAttribute('onclick');
                    var isFocusable = el.hasAttribute('tabindex') && el.getAttribute('tabindex') !== '-1';
                    var style = window.getComputedStyle(el);
                    var scrollData = checkScrollable(el, style);

                    if (isNativeInteractive || isAriaInteractive || hasClick || isFocusable || scrollData) {
                        var rect = el.getBoundingClientRect();

                        if (isElementVisible(el, rect, style)) {
                            var inputType = (el.getAttribute('type') || '').toLowerCase();
                            var autoComplete = (el.getAttribute('autocomplete') || '').toLowerCase();
                            var fieldHint = ((el.getAttribute('name') || '') + ' ' + (el.id || '')).toLowerCase();
                            var isSensitive = inputType === 'password'
                                || el.getAttribute('data-mask') === 'true'
                                || el.getAttribute('data-sensitive') === 'true'
                                || hasSensitiveHint(autoComplete + ' ' + fieldHint);

                            var rawVal = (el.value && typeof el.value === 'string') ? el.value.trim() : '';
                            var safeVal = (isSensitive && rawVal.length > 0) ? '[MASKED]' : rawVal;
                            var text = (safeVal || el.getAttribute('aria-label') || el.getAttribute('placeholder') || el.getAttribute('title') || el.innerText || '').trim();
                            if (text.length > 80) text = text.substring(0, 77) + '...';

                            var refId = '@v' + rev + ':e' + idCounter;
                            var idxStr = String(idCounter);

                            // Deterministic live DOM stamping for robust selection & visual alignment
                            try {
                                el.setAttribute('data-minicode-ref', refId);
                                el.setAttribute('data-minicode-idx', idxStr);
                            } catch (_) {}

                            var attrs = {};
                            if (el.id) attrs.id = el.id;
                            if (el.getAttribute('name')) attrs.name = el.getAttribute('name');
                            if (inputType) attrs.type = inputType;
                            if (el.getAttribute('placeholder')) attrs.placeholder = el.getAttribute('placeholder');
                            if (el.getAttribute('href')) attrs.href = el.getAttribute('href');
                            if (el.getAttribute('data-testid')) attrs.data_testid = el.getAttribute('data-testid');
                            if (isSensitive) attrs.masked = 'true';

                            var effectiveRole = (scrollData && !isNativeInteractive && !isAriaInteractive) ? 'scrollable' : role;

                            results.push({
                                tag: tag,
                                role: effectiveRole,
                                name: text,
                                selector: getShortSelector(el),
                                x: Math.round(rect.x),
                                y: Math.round(rect.y),
                                width: Math.round(rect.width),
                                height: Math.round(rect.height),
                                is_shadow_dom: inShadow,
                                is_scrollable: Boolean(scrollData),
                                scroll_data: scrollData,
                                attributes: attrs
                            });
                            idCounter++;
                        }
                    }
                }
            }

            walk(document, false);
            return results;
        },

        // Resolves target DOM node by stamped ref (@v1:e7), numeric index, verified selector, ID/attribute, or text content
        resolveElement: function(targetRef, targetName, selector) {
            var clean = targetRef ? String(targetRef).trim() : '';
            var isVersionedRef = clean.indexOf('@v') === 0;

            function matchesTargetName(candidate) {
                if (!candidate) return false;
                if (!targetName || targetName.trim().length === 0) return true;
                var expected = targetName.trim().toLowerCase();
                if (expected === clean.toLowerCase()) return true;
                var actual = (
                    candidate.innerText ||
                    candidate.value ||
                    candidate.getAttribute('aria-label') ||
                    candidate.getAttribute('title') ||
                    candidate.getAttribute('placeholder') ||
                    candidate.getAttribute('name') ||
                    ''
                ).trim().toLowerCase();
                return actual.length === 0 || actual === expected || actual.includes(expected) || expected.includes(actual);
            }

            function extractTrailingIndex(refStr) {
                if (!refStr) return null;
                var colonPos = refStr.lastIndexOf(':e');
                var tail = colonPos >= 0 ? refStr.slice(colonPos + 2) : (refStr.charAt(0) === 'e' ? refStr.slice(1) : refStr);
                if (!tail) return null;
                for (var k = 0; k < tail.length; k++) {
                    var code = tail.charCodeAt(k);
                    if (code < 48 || code > 57) return null;
                }
                return tail;
            }

            // 1. Live stamped DOM reference attributes take highest priority when targetRef is provided
            if (clean.length > 0) {
                try {
                    var byRef = document.querySelector('[data-minicode-ref="' + CSS.escape(clean) + '"]');
                    if (byRef && matchesTargetName(byRef)) return byRef;
                } catch (_) {}

                var idxDigits = extractTrailingIndex(clean);
                if (idxDigits) {
                    var byIdx = document.querySelector('[data-minicode-idx="' + idxDigits + '"]');
                    if (byIdx && matchesTargetName(byIdx)) return byIdx;
                }
            }

            // 2. Explicit CSS selector (direct selector call when clean is empty, or name-verified selector_hint when accompanying a ref)
            if (selector) {
                try {
                    var bySel = document.querySelector(selector);
                    if (bySel && (clean.length === 0 || matchesTargetName(bySel))) return bySel;
                } catch (_) {}
            }

            // 3. ID & exact attribute match (for non-@v refs)
            if (clean.length > 0 && !isVersionedRef) {
                var idClean = clean.charAt(0) === '#' ? clean.slice(1) : clean;
                var byId = document.getElementById(idClean);
                if (byId) return byId;

                try {
                    var byAttr = document.querySelector('[name="' + CSS.escape(clean) + '"], [data-testid="' + CSS.escape(clean) + '"], [aria-label="' + CSS.escape(clean) + '"]');
                    if (byAttr) return byAttr;
                } catch (_) {}
            }

            // 4. Intelligent fuzzy match by targetName among interactive elements
            if (targetName && targetName.trim().length > 0 && targetName.trim() !== clean) {
                var query = targetName.trim().toLowerCase();
                var candidates = Array.from(document.querySelectorAll('button, a, input, select, textarea, [role="button"], [role="link"], [role="tab"], [onclick]'));
                
                var exact = candidates.find(function(c) {
                    var t = (c.innerText || c.value || c.getAttribute('aria-label') || '').trim().toLowerCase();
                    return t === query;
                });
                if (exact) return exact;

                var contains = candidates.find(function(c) {
                    var t = (c.innerText || c.value || c.getAttribute('aria-label') || '').trim().toLowerCase();
                    return t.length > 0 && (t.includes(query) || query.includes(t));
                });
                if (contains) return contains;
            }

            // 5. Pierce Shadow DOMs for matching target
            var shadowIdx = extractTrailingIndex(clean);
            function searchShadow(root) {
                var els = root.querySelectorAll ? Array.from(root.querySelectorAll('*')) : [];
                for (var i = 0; i < els.length; i++) {
                    var e = els[i];
                    if (clean.length > 0) {
                        if (e.getAttribute('data-minicode-ref') === clean && matchesTargetName(e)) return e;
                        if (shadowIdx && e.getAttribute('data-minicode-idx') === shadowIdx && matchesTargetName(e)) return e;
                        if (!isVersionedRef && (e.id === clean || e.getAttribute('data-testid') === clean)) return e;
                    } else if (selector && e.matches) {
                        try {
                            if (e.matches(selector)) return e;
                        } catch (_) {}
                    }
                    if (e.shadowRoot) {
                        var found = searchShadow(e.shadowRoot);
                        if (found) return found;
                    }
                }
                return null;
            }
            return searchShadow(document);
        },

        // Injects simulator HUD aura into DOM if not already present (Option B: Neon Rim Cyber Glass)
        injectSimulatorAura: function(statusText, themeColor) {
            var color = themeColor || '#a277ff';
            var rgbColor = (function(hex) {
                try {
                    var c = hex.replace('#', '');
                    if (c.length === 3) c = c.split('').map(function(x){ return x + x; }).join('');
                    var num = parseInt(c, 16);
                    return ((num >> 16) & 255) + ', ' + ((num >> 8) & 255) + ', ' + (num & 255);
                } catch (_) {
                    return '162, 119, 255';
                }
            })(color);

            var existing = document.getElementById('minicode-simulator-container');
            if (existing) {
                existing.style.setProperty('--minicode-theme-color', color);
                existing.style.setProperty('--minicode-theme-color-rgb', rgbColor);
                if (statusText) {
                    var txtEl = document.getElementById('minicode-pill-text');
                    if (txtEl) txtEl.innerText = statusText;
                }
                return true;
            }

            var style = document.createElement('style');
            style.id = 'minicode-simulator-styles';
            style.textContent = `
                @keyframes minicode-eq-1 { 0%, 100% { height: 4px; } 50% { height: 12px; } }
                @keyframes minicode-eq-2 { 0%, 100% { height: 12px; } 50% { height: 5px; } }
                @keyframes minicode-eq-3 { 0%, 100% { height: 7px; } 50% { height: 13px; } }
                @keyframes minicode-rail-wave {
                    0% { background-position: 0% 0%; }
                    100% { background-position: 0% 200%; }
                }
                @keyframes minicode-ripple-wave {
                    0% { transform: translate(-50%, -50%) scale(0.2); opacity: 0.9; }
                    100% { transform: translate(-50%, -50%) scale(2.6); opacity: 0; }
                }
                #minicode-simulator-container {
                    --minicode-theme-color: ${color};
                    --minicode-theme-color-rgb: ${rgbColor};
                    position: fixed;
                    inset: 0;
                    width: 100vw;
                    height: 100vh;
                    pointer-events: none;
                    z-index: 2147483645;
                    overflow: hidden;
                    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
                }
                /* Minimal vertical glow rails on left and right sides only */
                .minicode-aura-rail {
                    position: absolute;
                    top: 0;
                    bottom: 0;
                    width: 3.5px;
                    pointer-events: none;
                    z-index: 2147483645;
                    opacity: 0.16;
                    background: var(--minicode-theme-color, #a277ff);
                    box-shadow: 0 0 10px var(--minicode-theme-color, #a277ff);
                    transition: opacity 0.4s ease, box-shadow 0.4s ease;
                }
                #minicode-aura-left-rail {
                    left: 0;
                }
                #minicode-aura-right-rail {
                    right: 0;
                }
                /* Active animation when minicode executes an action in GUI */
                .minicode-aura-rail.active {
                    opacity: 0.92;
                    background: linear-gradient(
                        180deg,
                        transparent 0%,
                        var(--minicode-theme-color, #a277ff) 25%,
                        #ffffff 50%,
                        var(--minicode-theme-color, #a277ff) 75%,
                        transparent 100%
                    );
                    background-size: 100% 200%;
                    animation: minicode-rail-wave 1.1s cubic-bezier(0.4, 0, 0.2, 1) infinite;
                    box-shadow: 0 0 16px var(--minicode-theme-color, #a277ff), 0 0 32px var(--minicode-theme-color, #a277ff);
                }
                #minicode-simulator-cursor {
                    position: absolute;
                    left: 50%;
                    top: 50%;
                    width: 30px;
                    height: 30px;
                    z-index: 2147483647;
                    pointer-events: none;
                    transition: left 0.28s cubic-bezier(0.2, 0, 0, 1), top 0.28s cubic-bezier(0.2, 0, 0, 1);
                    filter: drop-shadow(0 4px 10px rgba(0, 0, 0, 0.55));
                    transform: translate(-3px, -3px);
                }
                #minicode-cursor-ripple {
                    position: absolute;
                    width: 44px;
                    height: 44px;
                    border-radius: 50%;
                    border: 2px solid var(--minicode-theme-color, #a277ff);
                    background: radial-gradient(circle, var(--minicode-theme-color, #a277ff) 0%, transparent 70%);
                    pointer-events: none;
                    opacity: 0;
                    z-index: 2147483646;
                }
                #minicode-cursor-ripple.active {
                    animation: minicode-ripple-wave 0.5s ease-out forwards;
                }
                /* Option B: Neon Rim Cyber Glass status pill with responsive light/dark themes */
                #minicode-simulator-pill {
                    position: absolute;
                    bottom: 22px;
                    left: 50%;
                    transform: translateX(-50%);
                    background: rgba(10, 12, 20, 0.82);
                    backdrop-filter: blur(24px) saturate(200%);
                    -webkit-backdrop-filter: blur(24px) saturate(200%);
                    border: 1.5px solid var(--minicode-theme-color, #a277ff);
                    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.65), 0 0 18px rgba(var(--minicode-theme-color-rgb, 162, 119, 255), 0.35), inset 0 0 12px rgba(var(--minicode-theme-color-rgb, 162, 119, 255), 0.15);
                    border-radius: 12px;
                    padding: 7px 18px;
                    display: flex;
                    align-items: center;
                    gap: 12px;
                    color: #ffffff;
                    font-size: 13px;
                    font-weight: 500;
                    z-index: 2147483646;
                    pointer-events: auto;
                    user-select: none;
                    transition: all 0.25s ease;
                }
                @media (prefers-color-scheme: light) {
                    #minicode-simulator-pill {
                        background: rgba(248, 250, 255, 0.88);
                        border: 1.5px solid var(--minicode-theme-color, #a277ff);
                        box-shadow: 0 8px 28px rgba(0, 0, 0, 0.12), 0 0 14px rgba(var(--minicode-theme-color-rgb, 162, 119, 255), 0.25);
                        color: #0f172a;
                    }
                }
                .minicode-equalizer {
                    display: flex;
                    align-items: flex-end;
                    gap: 2.5px;
                    height: 12px;
                }
                .minicode-equalizer-bar {
                    width: 2.5px;
                    background: var(--minicode-theme-color, #a277ff);
                    border-radius: 2px;
                    transition: height 0.2s ease;
                }
                .minicode-equalizer-bar:nth-child(1) { height: 6px; }
                .minicode-equalizer-bar:nth-child(2) { height: 11px; }
                .minicode-equalizer-bar:nth-child(3) { height: 8px; }

                #minicode-simulator-pill.active .minicode-equalizer-bar:nth-child(1) { animation: minicode-eq-1 0.7s infinite; }
                #minicode-simulator-pill.active .minicode-equalizer-bar:nth-child(2) { animation: minicode-eq-2 0.5s infinite; }
                #minicode-simulator-pill.active .minicode-equalizer-bar:nth-child(3) { animation: minicode-eq-3 0.8s infinite; }

                #minicode-pill-text {
                    letter-spacing: -0.01em;
                    max-width: 440px;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                }
                #minicode-pill-stop {
                    background: rgba(239, 68, 68, 0.15);
                    border: 1px solid rgba(239, 68, 68, 0.4);
                    color: #ef4444;
                    font-size: 10px;
                    font-weight: 600;
                    cursor: pointer;
                    padding: 2px 6px;
                    border-radius: 4px;
                    display: inline-flex;
                    align-items: center;
                    gap: 3px;
                    opacity: 0.85;
                    transition: background 0.2s ease, opacity 0.2s ease;
                }
                #minicode-pill-stop:hover {
                    background: rgba(239, 68, 68, 0.3);
                    opacity: 1;
                }
            `;
            document.head.appendChild(style);

            var container = document.createElement('div');
            container.id = 'minicode-simulator-container';
            container.setAttribute('data-minicode-ignore', 'true');
            container.setAttribute('aria-hidden', 'true');
            container.style.setProperty('--minicode-theme-color', color);
            container.style.setProperty('--minicode-theme-color-rgb', rgbColor);

            container.innerHTML = `
                <div id="minicode-aura-left-rail" class="minicode-aura-rail"></div>
                <div id="minicode-aura-right-rail" class="minicode-aura-rail"></div>
                <div id="minicode-cursor-ripple"></div>
                <svg id="minicode-simulator-cursor" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <path d="M5.5 3.5L24.5 16.5L15.5 18.5L12 27.5L5.5 3.5Z" fill="var(--minicode-theme-color, #a277ff)" stroke="#ffffff" stroke-width="1.8" stroke-linejoin="round"/>
                </svg>
                <div id="minicode-simulator-pill">
                    <div class="minicode-equalizer">
                        <div class="minicode-equalizer-bar"></div>
                        <div class="minicode-equalizer-bar"></div>
                        <div class="minicode-equalizer-bar"></div>
                    </div>
                    <span id="minicode-pill-text">${statusText ? String(statusText).split('<').join('&lt;') : 'AI Agent Active'}</span>
                    <span id="minicode-pill-stop" title="Halt Agent">HALT ■</span>
                </div>
            `;
            document.body.appendChild(container);
            window.__minicode_halted = false;
            var stopBtn = document.getElementById('minicode-pill-stop');
            if (stopBtn) {
                stopBtn.addEventListener('click', function(ev) {
                    ev.stopPropagation();
                    window.__minicode_halted = true;
                    var txtEl = document.getElementById('minicode-pill-text');
                    if (txtEl) txtEl.innerText = 'Agent Halted by User';
                    var l = document.getElementById('minicode-aura-left-rail');
                    var r = document.getElementById('minicode-aura-right-rail');
                    var p = document.getElementById('minicode-simulator-pill');
                    if (l) l.classList.remove('active');
                    if (r) r.classList.remove('active');
                    if (p) p.classList.remove('active');
                });
            }
            return true;
        },

        // Updates simulator state: updates pill message, glides cursor, triggers ripple, activates rail wave & equalizer
        updateSimulatorState: function(actionText, targetX, targetY, isClick, themeColor) {
            if (!document.getElementById('minicode-simulator-container')) return;

            var txtEl = document.getElementById('minicode-pill-text');
            if (txtEl && actionText) txtEl.innerText = actionText;

            // Trigger active rail animation wave and equalizer bounce
            var leftRail = document.getElementById('minicode-aura-left-rail');
            var rightRail = document.getElementById('minicode-aura-right-rail');
            var pill = document.getElementById('minicode-simulator-pill');
            if (leftRail) leftRail.classList.add('active');
            if (rightRail) rightRail.classList.add('active');
            if (pill) pill.classList.add('active');
            if (window.__minicode_rail_timer) clearTimeout(window.__minicode_rail_timer);
            window.__minicode_rail_timer = setTimeout(function() {
                var l = document.getElementById('minicode-aura-left-rail');
                var r = document.getElementById('minicode-aura-right-rail');
                var p = document.getElementById('minicode-simulator-pill');
                if (l) l.classList.remove('active');
                if (r) r.classList.remove('active');
                if (p) p.classList.remove('active');
            }, 1800);

            var cursor = document.getElementById('minicode-simulator-cursor');
            if (cursor && typeof targetX === 'number' && typeof targetY === 'number') {
                cursor.style.left = targetX + 'px';
                cursor.style.top = targetY + 'px';
            }

            if (isClick && typeof targetX === 'number' && typeof targetY === 'number') {
                var ripple = document.getElementById('minicode-cursor-ripple');
                if (ripple) {
                    ripple.classList.remove('active');
                    ripple.style.left = targetX + 'px';
                    ripple.style.top = targetY + 'px';
                    void ripple.offsetWidth; // Force reflow to re-trigger CSS keyframes
                    ripple.classList.add('active');
                }
            }
            return true;
        },

        // Clears the simulator aura container and styles from DOM
        clearSimulatorAura: function() {
            window.__minicode_halted = false;
            var c = document.getElementById('minicode-simulator-container');
            if (c) c.remove();
            var s = document.getElementById('minicode-simulator-styles');
            if (s) s.remove();
            return true;
        },

        // Robustly clicks an element: scrolls into view, updates aura, and dispatches full W3C sequence
        clickElement: function(targetRef, targetName, selector) {
            if (window.__minicode_halted) {
                return JSON.stringify({ ok: false, halted: true, error: 'Execution halted by user via Simulator Mask HALT button' });
            }
            var el = this.resolveElement(targetRef, targetName, selector);
            if (!el) {
                return JSON.stringify({ ok: false, error: 'Element matching "' + (targetRef || selector || targetName || '') + '" not found in DOM' });
            }

            try {
                el.scrollIntoView({ behavior: 'instant', block: 'center', inline: 'center' });
            } catch (_) {}

            var rect = el.getBoundingClientRect();
            var cx = Math.round(rect.left + rect.width / 2);
            var cy = Math.round(rect.top + rect.height / 2);

            var idx = el.getAttribute('data-minicode-idx') || '';
            var name = (el.innerText || el.value || el.getAttribute('aria-label') || targetName || '').trim();
            if (name.length > 30) name = name.substring(0, 27) + '...';

            var actionLabel = idx ? ('Clicking element [' + idx + ']' + (name ? ' "' + name + '"' : '') + '...') : ('Clicking <' + el.tagName.toLowerCase() + '>' + (name ? ' "' + name + '"' : '') + '...');
            this.updateSimulatorState(actionLabel, cx, cy, true);

            // Filter out simulator overlays so hitTarget never hits the AI cursor or ripple
            var hitTarget = el;
            if (document.elementFromPoint) {
                var candidate = document.elementFromPoint(cx, cy);
                if (candidate && !candidate.closest('#minicode-simulator-container') && candidate !== document.body && candidate !== document.documentElement) {
                    hitTarget = candidate;
                }
            }

            // If valid coordinates exist, native CDP mouse_click_at(cx, cy) will deliver
            // the authentic W3C event sequence with isTrusted: true without double-firing.
            // Only perform synthetic DOM click fallback if element has no rendered coordinates.
            if (cx <= 0 && cy <= 0) {
                var pointerOpts = { bubbles: true, cancelable: true, clientX: 0, clientY: 0, pointerType: 'mouse', view: window };
                var mouseOpts = { bubbles: true, cancelable: true, clientX: 0, clientY: 0, button: 0, view: window };
                try { hitTarget.dispatchEvent(new PointerEvent('pointerdown', pointerOpts)); } catch (_) {}
                try { hitTarget.dispatchEvent(new MouseEvent('mousedown', mouseOpts)); } catch (_) {}
                try { if (typeof hitTarget.focus === 'function') hitTarget.focus(); } catch (_) {}
                try { hitTarget.dispatchEvent(new PointerEvent('pointerup', pointerOpts)); } catch (_) {}
                try { hitTarget.dispatchEvent(new MouseEvent('mouseup', mouseOpts)); } catch (_) {}
                try { hitTarget.dispatchEvent(new MouseEvent('click', mouseOpts)); } catch (_) {}
                if (hitTarget !== el) {
                    try { el.dispatchEvent(new MouseEvent('click', mouseOpts)); } catch (_) {}
                }
                try {
                    if (typeof el.click === 'function') {
                        el.click();
                    } else if (typeof hitTarget.click === 'function') {
                        hitTarget.click();
                    }
                } catch (_) {}
            }

            // Re-read element name / value after click mutation
            var updatedName = (el.innerText || el.value || el.getAttribute('aria-label') || name).trim();
            if (updatedName.length > 80) updatedName = updatedName.substring(0, 77) + '...';

            return JSON.stringify({
                ok: true,
                x: cx,
                y: cy,
                ref: el.getAttribute('data-minicode-ref') || targetRef,
                idx: idx,
                tag: el.tagName.toLowerCase(),
                name: updatedName
            });
        },

        // Types text into an input, textarea, or contenteditable element, updating simulator aura & React controlled state
        inputText: function(targetRef, targetName, selector, text) {
            if (window.__minicode_halted) {
                return JSON.stringify({ ok: false, halted: true, error: 'Execution halted by user via Simulator Mask HALT button' });
            }
            var el = this.resolveElement(targetRef, targetName, selector);
            if (!el) {
                return JSON.stringify({ ok: false, error: 'Input element matching "' + (targetRef || selector || targetName || '') + '" not found in DOM' });
            }

            try {
                el.scrollIntoView({ behavior: 'instant', block: 'center', inline: 'center' });
            } catch (_) {}

            var rect = el.getBoundingClientRect();
            var cx = Math.round(rect.left + rect.width / 2);
            var cy = Math.round(rect.top + rect.height / 2);
            var idx = el.getAttribute('data-minicode-idx') || '';
            var label = (el.getAttribute('placeholder') || el.getAttribute('aria-label') || el.getAttribute('name') || targetName || '').trim();
            if (label.length > 26) label = label.substring(0, 23) + '...';
            var actionLabel = idx ? ('Typing into [' + idx + ']' + (label ? ' "' + label + '"' : '') + '...') : ('Typing into <' + el.tagName.toLowerCase() + '>...');
            this.updateSimulatorState(actionLabel, cx, cy, true);

            try { if (typeof el.focus === 'function') el.focus(); } catch (_) {}

            var proto = el.tagName === 'TEXTAREA' ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype;
            var desc = proto ? Object.getOwnPropertyDescriptor(proto, 'value') : null;
            var nativeSetter = desc ? desc.set : null;
            if (nativeSetter && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA')) {
                nativeSetter.call(el, text);
            } else if (el.isContentEditable) {
                el.textContent = text;
            } else {
                el.value = text;
            }

            try { el.dispatchEvent(new Event('input', { bubbles: true })); } catch (_) {}
            try { el.dispatchEvent(new Event('change', { bubbles: true })); } catch (_) {}

            return JSON.stringify({
                ok: true,
                x: cx,
                y: cy,
                ref: el.getAttribute('data-minicode-ref') || targetRef,
                idx: idx,
                tag: el.tagName.toLowerCase(),
                name: label
            });
        },

        // Selects an option in a <select> dropdown by visible text or value, updating simulator aura
        selectOption: function(targetRef, targetName, selector, optionText) {
            if (window.__minicode_halted) {
                return JSON.stringify({ ok: false, halted: true, error: 'Execution halted by user via Simulator Mask HALT button' });
            }
            var el = this.resolveElement(targetRef, targetName, selector);
            if (el && el.tagName.toLowerCase() !== 'select' && el.querySelector) {
                el = el.querySelector('select') || el;
            }
            if (!el || el.tagName.toLowerCase() !== 'select') {
                return JSON.stringify({ ok: false, error: 'Select element matching "' + (targetRef || selector || targetName || '') + '" not found in DOM' });
            }

            try {
                el.scrollIntoView({ behavior: 'instant', block: 'center', inline: 'center' });
            } catch (_) {}

            var rect = el.getBoundingClientRect();
            var cx = Math.round(rect.left + rect.width / 2);
            var cy = Math.round(rect.top + rect.height / 2);
            var idx = el.getAttribute('data-minicode-idx') || '';
            this.updateSimulatorState('Selecting "' + optionText + '"' + (idx ? ' in [' + idx + ']' : '') + '...', cx, cy, true);

            var options = Array.from(el.options || []);
            var needle = (optionText || '').trim().toLowerCase();
            var found = options.find(function(o) {
                return o.text.trim().toLowerCase() === needle || o.value.trim().toLowerCase() === needle;
            }) || options.find(function(o) {
                return o.text.trim().toLowerCase().includes(needle) || o.value.trim().toLowerCase().includes(needle);
            });

            if (!found) {
                return JSON.stringify({ ok: false, error: 'Option "' + optionText + '" not found in <select>' });
            }

            el.value = found.value;
            found.selected = true;
            try { el.dispatchEvent(new Event('input', { bubbles: true })); } catch (_) {}
            try { el.dispatchEvent(new Event('change', { bubbles: true })); } catch (_) {}

            return JSON.stringify({
                ok: true,
                x: cx,
                y: cy,
                ref: el.getAttribute('data-minicode-ref') || targetRef,
                selected_text: found.text.trim(),
                selected_value: found.value
            });
        },

        // Hovers over an element, updating simulator aura cursor and dispatching pointer/mouse hover events
        hoverElement: function(targetRef, targetName, selector) {
            if (window.__minicode_halted) {
                return JSON.stringify({ ok: false, halted: true, error: 'Execution halted by user via Simulator Mask HALT button' });
            }
            var el = this.resolveElement(targetRef, targetName, selector);
            if (!el) {
                return JSON.stringify({ ok: false, error: 'Element matching "' + (targetRef || selector || targetName || '') + '" not found in DOM' });
            }

            try {
                el.scrollIntoView({ behavior: 'instant', block: 'center', inline: 'center' });
            } catch (_) {}

            var rect = el.getBoundingClientRect();
            var cx = Math.round(rect.left + rect.width / 2);
            var cy = Math.round(rect.top + rect.height / 2);
            var idx = el.getAttribute('data-minicode-idx') || '';
            var name = (el.innerText || el.getAttribute('aria-label') || targetName || '').trim();
            if (name.length > 26) name = name.substring(0, 23) + '...';
            this.updateSimulatorState('Hovering ' + (idx ? '[' + idx + '] ' : '') + (name ? '"' + name + '"' : '<' + el.tagName.toLowerCase() + '>'), cx, cy, false);

            var opts = { bubbles: true, cancelable: true, clientX: cx, clientY: cy, view: window };
            try { el.dispatchEvent(new PointerEvent('pointerover', opts)); } catch (_) {}
            try { el.dispatchEvent(new PointerEvent('pointerenter', Object.assign({}, opts, { bubbles: false }))); } catch (_) {}
            try { el.dispatchEvent(new MouseEvent('mouseover', opts)); } catch (_) {}
            try { el.dispatchEvent(new MouseEvent('mouseenter', Object.assign({}, opts, { bubbles: false }))); } catch (_) {}
            try { el.dispatchEvent(new MouseEvent('mousemove', opts)); } catch (_) {}

            return JSON.stringify({
                ok: true,
                x: cx,
                y: cy,
                ref: el.getAttribute('data-minicode-ref') || targetRef,
                tag: el.tagName.toLowerCase(),
                name: name
            });
        },

        // Scrolls viewport with fallback to largest scrollable container in DOM
        scrollPage: function(direction, pixels) {
            if (window.__minicode_halted) {
                return JSON.stringify({ ok: false, halted: true, error: 'Execution halted by user via Simulator Mask HALT button' });
            }
            var dir = (direction || 'down').toLowerCase();
            var vh = window.innerHeight || 800;
            var step = pixels || Math.round(vh * 0.75);

            this.updateSimulatorState('Scrolling ' + dir + '...', Math.round(window.innerWidth / 2), Math.round(vh / 2), false);

            var initialY = window.scrollY || window.pageYOffset || (document.documentElement && document.documentElement.scrollTop) || 0;
            var scrolled = false;

            if (dir === 'top') {
                window.scrollTo({ top: 0, behavior: 'instant' });
                if (document.documentElement) document.documentElement.scrollTop = 0;
                if (document.body) document.body.scrollTop = 0;
                scrolled = true;
            } else if (dir === 'bottom') {
                var maxScroll = Math.max(document.body.scrollHeight, document.documentElement.scrollHeight || 0);
                window.scrollTo({ top: maxScroll, behavior: 'instant' });
                if (document.documentElement) document.documentElement.scrollTop = maxScroll;
                if (document.body) document.body.scrollTop = maxScroll;
                scrolled = true;
            } else {
                var delta = (dir === 'up' || dir === 'pageup') ? -step : step;
                window.scrollBy({ top: delta, behavior: 'instant' });
                if (document.documentElement) document.documentElement.scrollTop += delta;
                if (document.body) document.body.scrollTop += delta;

                var newY = window.scrollY || window.pageYOffset || (document.documentElement && document.documentElement.scrollTop) || 0;
                if (Math.abs(newY - initialY) > 5) {
                    scrolled = true;
                } else {
                    // Container fallback: search for scrollable containers in DOM
                    var allEls = Array.from(document.querySelectorAll('*'));
                    var scrollableContainers = [];

                    for (var i = 0; i < allEls.length; i++) {
                        var el = allEls[i];
                        if (el === document.body || el === document.documentElement) continue;
                        if (el.scrollHeight > el.clientHeight + 10) {
                            var style = window.getComputedStyle(el);
                            if (SCROLL_MODES[style.overflowY] === 1) {
                                scrollableContainers.push({
                                    el: el,
                                    area: el.clientWidth * el.clientHeight
                                });
                            }
                        }
                    }

                    scrollableContainers.sort(function(a, b) { return b.area - a.area; });

                    if (scrollableContainers.length > 0) {
                        var targetCont = scrollableContainers[0].el;
                        targetCont.scrollBy({ top: delta, behavior: 'instant' });
                        scrolled = true;
                    }
                }
            }

            return JSON.stringify({
                ok: true,
                direction: dir,
                scrolled: scrolled,
                new_scroll_y: window.scrollY || window.pageYOffset || (document.documentElement && document.documentElement.scrollTop) || 0
            });
        },

        // Calculates viewport and total page scroll metrics
        getPageMetrics: function() {
            var vw = window.innerWidth;
            var vh = window.innerHeight;
            var pw = Math.max(document.documentElement.scrollWidth, document.body.scrollWidth || 0);
            var ph = Math.max(document.documentElement.scrollHeight, document.body.scrollHeight || 0);
            var sx = window.scrollX || window.pageXOffset || 0;
            var sy = window.scrollY || window.pageYOffset || 0;
            var pb = Math.max(0, ph - (vh + sy));
            return {
                viewport_width: vw,
                viewport_height: vh,
                page_width: pw,
                page_height: ph,
                scroll_x: sx,
                scroll_y: sy,
                pixels_above: sy,
                pixels_below: pb,
                pages_above: vh > 0 ? Number((sy / vh).toFixed(2)) : 0,
                pages_below: vh > 0 ? Number((pb / vh).toFixed(2)) : 0,
                total_pages: vh > 0 ? Number((ph / vh).toFixed(2)) : 0,
                current_page_position: Number((sy / Math.max(1, ph - vh)).toFixed(3))
            };
        },

        // Injects high-contrast indexed badge overlays on interactive elements for visual verification
        injectVisualBadges: function() {
            var existing = document.getElementById('minicode-highlight-container');
            if (existing) existing.remove();

            var container = document.createElement('div');
            container.id = 'minicode-highlight-container';
            container.setAttribute('data-minicode-ignore', 'true');
            container.style.cssText = 'position:absolute;top:0;left:0;width:100%;height:100%;pointer-events:none;z-index:2147483640;';

            var items = this.scanInteractables();
            var colors = ['#2563eb', '#16a34a', '#d97706', '#dc2626', '#9333ea', '#0891b2'];
            var count = 0;

            for (var i = 0; i < items.length; i++) {
                var it = items[i];
                var color = colors[i % colors.length];
                var box = document.createElement('div');
                box.style.cssText = 'position:absolute;left:' + (it.x + window.scrollX) + 'px;top:' + (it.y + window.scrollY) + 'px;width:' + it.width + 'px;height:' + it.height + 'px;border:2px solid ' + color + ';pointer-events:none;box-sizing:border-box;border-radius:3px;box-shadow:0 0 4px rgba(0,0,0,0.3);';

                var badge = document.createElement('span');
                badge.innerText = '[' + (i + 1) + ']';
                badge.style.cssText = 'position:absolute;top:-13px;left:-2px;background:' + color + ';color:#fff;font-size:10px;font-weight:bold;padding:1px 3px;border-radius:2px;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;line-height:1;box-shadow:0 1px 2px rgba(0,0,0,0.4);';

                box.appendChild(badge);
                container.appendChild(box);
                count++;
            }
            document.body.appendChild(container);
            return count;
        },

        // Removes visual badge overlay container from the DOM
        clearVisualBadges: function() {
            var existing = document.getElementById('minicode-highlight-container');
            if (existing) {
                existing.remove();
                return true;
            }
            return false;
        },

        // Audits broken images, broken links, form accessibility, and page diagnostics
        auditPage: function() {
            var brokenImages = [];
            var images = document.querySelectorAll('img');
            images.forEach(function(img) {
                var src = img.getAttribute('src') || '';
                if (!src || (img.complete && img.naturalWidth === 0)) {
                    brokenImages.push(src ? src : '<img without src>');
                }
            });

            var brokenLinks = [];
            var links = document.querySelectorAll('a');
            links.forEach(function(a) {
                var href = (a.getAttribute('href') || '').trim();
                var text = (a.innerText || a.getAttribute('aria-label') || '').trim();
                if (!href || href === '#' || href.toLowerCase().indexOf('javascript:void') === 0) {
                    brokenLinks.push('"' + text + '" -> href="' + href + '"');
                }
            });

            var formsSummary = [];
            var forms = document.querySelectorAll('form');
            forms.forEach(function(f, idx) {
                var inputs = f.querySelectorAll('input, select, textarea');
                var unlabelled = 0;
                inputs.forEach(function(input) {
                    var hasLabel = input.id && document.querySelector('label[for="' + CSS.escape(input.id) + '"]');
                    var hasAria = input.getAttribute('aria-label') || input.getAttribute('placeholder');
                    if (!hasLabel && !hasAria && input.type !== 'hidden' && input.type !== 'submit') {
                        unlabelled++;
                    }
                });
                var action = f.getAttribute('action') || '(current page)';
                var method = (f.getAttribute('method') || 'GET').toUpperCase();
                formsSummary.push('Form #' + (idx + 1) + ' [' + method + ' ' + action + ']: ' + inputs.length + ' fields' + (unlabelled > 0 ? ' (' + unlabelled + ' missing labels)' : ''));
            });

            var h1Count = document.querySelectorAll('h1').length;
            var title = document.title || '';
            var hasViewport = Boolean(document.querySelector('meta[name="viewport"]'));

            // Taste-Skill v2.1 Layout & Typography Diagnostics
            var layoutWarnings = [];
            try {
                var vw = window.innerWidth || 1280;
                var vh = window.innerHeight || 800;
                var overflowX = Math.max(0, (document.documentElement ? document.documentElement.scrollWidth : 0) - vw);
                if (overflowX > 4) {
                    layoutWarnings.push('[horizontal-overflow] Page overflows horizontally by ' + overflowX + 'px at ' + vw + 'px viewport width.');
                }
                var h1El = document.querySelector('h1');
                if (h1El && document.createTreeWalker && document.createRange) {
                    var walker = document.createTreeWalker(h1El, NodeFilter.SHOW_TEXT);
                    var range = document.createRange();
                    var lineTops = [];
                    var tNode;
                    while ((tNode = walker.nextNode())) {
                        if (!tNode.nodeValue || !tNode.nodeValue.trim()) continue;
                        range.selectNodeContents(tNode);
                        var rects = range.getClientRects();
                        for (var rIdx = 0; rIdx < rects.length; rIdx++) {
                            var rr = rects[rIdx];
                            if (rr.width < 2 || rr.height < 2) continue;
                            if (!lineTops.some(function(y) { return Math.abs(y - rr.top) < 6; })) {
                                lineTops.push(rr.top);
                            }
                        }
                    }
                    if (lineTops.length > 2) {
                        layoutWarnings.push('[headline-lines] Primary <h1> wraps across ' + lineTops.length + ' rendered lines (target <= 2 lines; tighten copy or clamp font-size).');
                    }
                }
                var sections = Array.from(document.querySelectorAll('section'));
                var prevBot = null;
                var paintedTags = { DIV: 1, ARTICLE: 1, ASIDE: 1, UL: 1, OL: 1, DL: 1 };
                sections.forEach(function(sec, idx) {
                    var cs = window.getComputedStyle ? window.getComputedStyle(sec) : null;
                    var sr = sec.getBoundingClientRect();
                    if (!cs || cs.display === 'none' || cs.visibility === 'hidden' || sr.width < vw * 0.65 || sr.height < 80) return;
                    var secId = sec.id ? ' (#' + sec.id + ')' : '';
                    var hEl = sec.querySelector('h1, h2, h3');
                    var hTxt = hEl && hEl.innerText ? ' "' + collapseSpaces(hEl.innerText).slice(0, 36) + '"' : '';
                    var secLabel = 'Section #' + (idx + 1) + secId + hTxt;
                    if (prevBot !== null && sr.top - prevBot >= vh * 0.45) {
                        layoutWarnings.push('[blank-gap] Vertical blank gap of ' + Math.round(sr.top - prevBot) + 'px before ' + secLabel + '.');
                    }
                    prevBot = Math.max(prevBot || 0, sr.bottom);
                    if (sr.height >= 220 && vw >= 768 && parseFloat(cs.opacity || '1') > 0.05) {
                        var kids = Array.from(sec.querySelectorAll('div, article, aside, ul, ol, dl, figure, blockquote, pre, code, li, dt, dd, th, td, label, input, textarea, select, span, h1, h2, h3, h4, h5, h6, p, button, a, img, svg, canvas, video, table, form, [data-ui]'));
                        var minX = vw, maxX = 0, cCount = 0;
                        kids.slice(0, 300).forEach(function(k) {
                            var kcs = window.getComputedStyle ? window.getComputedStyle(k) : null;
                            if (!kcs || kcs.display === 'none' || kcs.visibility === 'hidden' || kcs.pointerEvents === 'none') return;
                            if (paintedTags[k.tagName] && kcs.borderTopWidth === '0px' && kcs.backgroundColor === 'rgba(0, 0, 0, 0)') return;
                            var kr = k.getBoundingClientRect();
                            if (kr.width < 8 || kr.height < 8 || kr.width >= vw * 0.99) return;
                            minX = Math.min(minX, Math.max(0, kr.left));
                            maxX = Math.max(maxX, Math.min(vw, kr.right));
                            cCount++;
                        });
                        if (cCount >= 2 && maxX > minX) {
                            var leftM = minX, rightM = vw - maxX;
                            var emptyRatio = (leftM + rightM) / vw;
                            if (Math.abs(leftM - rightM) > vw * 0.28 && emptyRatio >= 0.42) {
                                layoutWarnings.push('[lopsided-section] ' + secLabel + ' leaves ' + Math.round(emptyRatio * 100) + '% of horizontal width empty on one side.');
                            }
                        }
                    }
                });
            } catch (_) {}

            return {
                title: title,
                has_viewport: hasViewport,
                h1_count: h1Count,
                broken_images: brokenImages,
                broken_links: brokenLinks,
                forms_summary: formsSummary,
                total_images: images.length,
                total_links: links.length,
                layout_warnings: layoutWarnings
            };
        },

        // Focuses optional target element and dispatches keyboard events + form/dialog/tab actions
        pressKey: function(key, targetRef, targetName, selector, modifiers) {
            if (window.__minicode_halted) {
                return JSON.stringify({ ok: false, halted: true, error: 'Execution halted by user via Simulator Mask HALT button' });
            }
            var el = (targetRef || selector) ? this.resolveElement(targetRef, targetName, selector) : (document.activeElement || document.body);
            if ((targetRef || selector) && !el) {
                return JSON.stringify({ ok: false, error: 'Element matching "' + (targetRef || selector || '') + '" not found in DOM' });
            }
            el = el || document.body;
            try { if (typeof el.focus === 'function') el.focus(); } catch (_) {}
            var rect = el.getBoundingClientRect ? el.getBoundingClientRect() : { left: 200, top: 200, width: 40, height: 24 };
            var cx = Math.round(rect.left + rect.width / 2);
            var cy = Math.round(rect.top + rect.height / 2);
            this.updateSimulatorState('Pressing "' + key + '"...', cx, cy, false);

            var mods = modifiers || 0;
            var evInit = {
                key: key,
                code: key,
                bubbles: true,
                cancelable: true,
                altKey: Boolean(mods & 1),
                ctrlKey: Boolean(mods & 2),
                metaKey: Boolean(mods & 4),
                shiftKey: Boolean(mods & 8)
            };
            var notCancelled = el.dispatchEvent(new KeyboardEvent('keydown', evInit));
            if (key.length === 1 && !(mods & 6)) {
                el.dispatchEvent(new KeyboardEvent('keypress', evInit));
            }
            el.dispatchEvent(new KeyboardEvent('keyup', evInit));
            if (notCancelled && key === 'Enter' && el.form) {
                try {
                    if (typeof el.form.requestSubmit === 'function') el.form.requestSubmit();
                    else el.form.submit();
                } catch (_) {}
            }
            return JSON.stringify({ ok: true, key: key, tag: (el.tagName || 'body').toLowerCase() });
        },

        // Toggles checkbox, radio, or role="switch"|"checkbox" element to desired boolean state
        setChecked: function(targetRef, targetName, selector, desiredChecked) {
            if (window.__minicode_halted) {
                return JSON.stringify({ ok: false, halted: true, error: 'Execution halted by user via Simulator Mask HALT button' });
            }
            var el = this.resolveElement(targetRef, targetName, selector);
            if (!el) {
                return JSON.stringify({ ok: false, error: 'Checkbox/switch matching "' + (targetRef || selector || '') + '" not found in DOM' });
            }
            try { el.scrollIntoView({ behavior: 'instant', block: 'center', inline: 'center' }); } catch (_) {}
            var rect = el.getBoundingClientRect();
            var cx = Math.round(rect.left + rect.width / 2);
            var cy = Math.round(rect.top + rect.height / 2);
            this.updateSimulatorState((desiredChecked ? 'Checking ' : 'Unchecking ') + (targetRef || selector || '<input>') + '...', cx, cy, true);

            var current = el.tagName === 'INPUT' ? Boolean(el.checked) : (el.getAttribute('aria-checked') === 'true');
            if (current !== Boolean(desiredChecked)) {
                if (typeof el.click === 'function') {
                    el.click();
                } else {
                    el.checked = Boolean(desiredChecked);
                    el.dispatchEvent(new Event('input', { bubbles: true }));
                    el.dispatchEvent(new Event('change', { bubbles: true }));
                }
            }
            return JSON.stringify({
                ok: true,
                x: cx,
                y: cy,
                checked: el.tagName === 'INPUT' ? Boolean(el.checked) : (el.getAttribute('aria-checked') === 'true')
            });
        },

        // Performs HTML5 DragEvent + W3C PointerEvent drag-and-drop between two elements or by (dx, dy) offset
        dragAndDrop: function(srcRef, srcSelector, dstRef, dstSelector, dx, dy) {
            if (window.__minicode_halted) {
                return JSON.stringify({ ok: false, halted: true, error: 'Execution halted by user via Simulator Mask HALT button' });
            }
            var src = this.resolveElement(srcRef, '', srcSelector);
            if (!src) {
                return JSON.stringify({ ok: false, error: 'Drag source "' + (srcRef || srcSelector || '') + '" not found' });
            }
            try { src.scrollIntoView({ behavior: 'instant', block: 'center' }); } catch (_) {}
            var sRect = src.getBoundingClientRect();
            var sx = Math.round(sRect.left + sRect.width / 2);
            var sy = Math.round(sRect.top + sRect.height / 2);

            var dst = (dstRef || dstSelector) ? this.resolveElement(dstRef, '', dstSelector) : null;
            var tx = sx + (typeof dx === 'number' ? dx : 0);
            var ty = sy + (typeof dy === 'number' ? dy : 0);
            if (dst) {
                var dRect = dst.getBoundingClientRect();
                tx = Math.round(dRect.left + dRect.width / 2);
                ty = Math.round(dRect.top + dRect.height / 2);
            } else {
                dst = document.elementFromPoint(tx, ty) || document.body;
            }

            this.updateSimulatorState('Dragging to (' + tx + ', ' + ty + ')...', tx, ty, true);
            var dt = null;
            try { dt = new DataTransfer(); } catch (_) {}

            try { src.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, clientX: sx, clientY: sy, buttons: 1 })); } catch (_) {}
            try { src.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, clientX: sx, clientY: sy, buttons: 1 })); } catch (_) {}
            try { src.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, clientX: sx, clientY: sy, dataTransfer: dt })); } catch (_) {}

            var steps = 5;
            for (var s = 1; s <= steps; s++) {
                var ix = Math.round(sx + ((tx - sx) * s) / steps);
                var iy = Math.round(sy + ((ty - sy) * s) / steps);
                try { dst.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, cancelable: true, clientX: ix, clientY: iy, buttons: 1 })); } catch (_) {}
                try { dst.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, cancelable: true, clientX: ix, clientY: iy, buttons: 1 })); } catch (_) {}
                try { dst.dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, clientX: ix, clientY: iy, dataTransfer: dt })); } catch (_) {}
            }

            try { dst.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, clientX: tx, clientY: ty, dataTransfer: dt })); } catch (_) {}
            try { src.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, clientX: tx, clientY: ty, dataTransfer: dt })); } catch (_) {}
            try { dst.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, cancelable: true, clientX: tx, clientY: ty })); } catch (_) {}
            try { dst.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, cancelable: true, clientX: tx, clientY: ty })); } catch (_) {}

            return JSON.stringify({ ok: true, from: [sx, sy], to: [tx, ty] });
        },

        // Dispatches double_click (`dblclick`) or right_click (`contextmenu`) on target element
        pointerAction: function(mode, targetRef, targetName, selector) {
            if (window.__minicode_halted) {
                return JSON.stringify({ ok: false, halted: true, error: 'Execution halted by user via Simulator Mask HALT button' });
            }
            var el = this.resolveElement(targetRef, targetName, selector);
            if (!el) {
                return JSON.stringify({ ok: false, error: 'Element matching "' + (targetRef || selector || '') + '" not found' });
            }
            try { el.scrollIntoView({ behavior: 'instant', block: 'center' }); } catch (_) {}
            var r = el.getBoundingClientRect();
            var cx = Math.round(r.left + r.width / 2);
            var cy = Math.round(r.top + r.height / 2);
            this.updateSimulatorState((mode === 'right_click' ? 'Right-clicking ' : 'Double-clicking ') + (targetRef || selector || 'element') + '...', cx, cy, true);

            if (mode === 'right_click') {
                el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, clientX: cx, clientY: cy, button: 2, buttons: 2 }));
                el.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, cancelable: true, clientX: cx, clientY: cy, button: 2 }));
                el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: cx, clientY: cy, button: 2 }));
            } else {
                el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, clientX: cx, clientY: cy, detail: 1 }));
                el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, clientX: cx, clientY: cy, detail: 2 }));
                el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true, clientX: cx, clientY: cy, detail: 2 }));
            }
            return JSON.stringify({ ok: true, x: cx, y: cy, mode: mode });
        }
    };
    return 'installed';
})()"###;

/// Scroll distance metrics for a scrollable container
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ScrollData {
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

impl ScrollData {
    /// Formats non-zero scrollable overflow directions into a compact badge string
    pub fn format_compact(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.top > 1.0 {
            parts.push(format!("↑{}px", self.top.round() as i64));
        }
        if self.bottom > 1.0 {
            parts.push(format!("↓{}px", self.bottom.round() as i64));
        }
        if self.left > 1.0 {
            parts.push(format!("←{}px", self.left.round() as i64));
        }
        if self.right > 1.0 {
            parts.push(format!("→{}px", self.right.round() as i64));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }
}

/// Page-level scroll position and viewport metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PageMetrics {
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub page_width: f64,
    pub page_height: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub pixels_above: f64,
    pub pixels_below: f64,
    pub pages_above: f64,
    pub pages_below: f64,
    pub total_pages: f64,
    pub current_page_position: f64,
}

impl PageMetrics {
    /// Formats viewport and scroll depth metrics into an agent-readable summary line
    pub fn format_summary(&self) -> String {
        format!(
            "Viewport: {}x{}px | Scroll Depth: {:.2} pages above ({}px), {:.2} pages below ({}px) | Total Height: {:.2} pages ({}px)",
            self.viewport_width.round() as i64,
            self.viewport_height.round() as i64,
            self.pages_above,
            self.pixels_above.round() as i64,
            self.pages_below,
            self.pixels_below.round() as i64,
            self.total_pages,
            self.page_height.round() as i64,
        )
    }
}

/// An in-page visual element grounded with computed coordinates and selectors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualElement {
    pub tag: String,
    pub role: String,
    pub name: String,
    pub selector: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_shadow_dom: bool,
    #[serde(default)]
    pub is_scrollable: bool,
    #[serde(default)]
    pub scroll_data: Option<ScrollData>,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
}

/// Raw audit payload returned by the in-page probe
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RawPageAudit {
    pub title: String,
    pub has_viewport: bool,
    pub h1_count: usize,
    pub broken_images: Vec<String>,
    pub broken_links: Vec<String>,
    pub forms_summary: Vec<String>,
    pub total_images: usize,
    pub total_links: usize,
    #[serde(default)]
    pub layout_warnings: Vec<String>,
}

/// Structured Quality Assurance audit report for a web page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaAuditReport {
    pub url: String,
    pub title: String,
    pub pass_count: usize,
    pub warn_count: usize,
    pub error_count: usize,
    pub broken_images: Vec<String>,
    pub broken_links: Vec<String>,
    pub console_errors: Vec<String>,
    pub network_errors: Vec<String>,
    pub forms_summary: Vec<String>,
    pub interactive_count: usize,
    pub recommendations: Vec<String>,
}

/// Native Rust controller managing in-page agent inspection and testing
pub struct PageAgent;

impl PageAgent {
    /// Injects the probe into the active page context if not already present
    pub async fn inject_probe(cdp: &CdpClient) -> Result<()> {
        if let Ok(existing) = cdp
            .evaluate_js(
                "typeof window.__minicode_page_agent === 'object' && window.__minicode_page_agent !== null",
            )
            .await
        {
            if existing.trim() == "true" {
                return Ok(());
            }
        }
        cdp.evaluate_js(PAGE_PROBE_JS).await?;
        Ok(())
    }

    /// Retrieves scroll metrics and viewport geometry for the current page
    pub async fn get_page_metrics(cdp: &CdpClient) -> Result<PageMetrics> {
        Self::inject_probe(cdp).await?;
        let res_json = cdp
            .evaluate_js("JSON.stringify(window.__minicode_page_agent.getPageMetrics())")
            .await?;
        let metrics: PageMetrics = serde_json::from_str(&res_json)
            .map_err(|e| ToolError::CommandExec(format!("Failed parsing page metrics: {}", e)))?;
        Ok(metrics)
    }

    /// Injects numbered visual badge overlays onto interactive elements for visual verification
    pub async fn inject_visual_badges(cdp: &CdpClient) -> Result<usize> {
        Self::inject_probe(cdp).await?;
        let count_str = cdp
            .evaluate_js("window.__minicode_page_agent.injectVisualBadges()")
            .await?;
        let count = count_str.trim().parse::<usize>().unwrap_or(0);
        Ok(count)
    }

    /// Clears visual badge overlays from the active page
    pub async fn clear_visual_badges(cdp: &CdpClient) -> Result<bool> {
        Self::inject_probe(cdp).await?;
        let res_str = cdp
            .evaluate_js("window.__minicode_page_agent.clearVisualBadges()")
            .await?;
        Ok(res_str.trim() == "true")
    }

    /// Injects the visual simulator aura, corner glows, AI cursor, and floating status pill
    pub async fn inject_simulator_aura(cdp: &CdpClient, status_text: Option<&str>) -> Result<()> {
        Self::inject_probe(cdp).await?;
        let status_json = serde_json::to_string(status_text.unwrap_or("AI Agent Active"))
            .unwrap_or_else(|_| "\"AI Agent Active\"".to_string());
        let script = format!(
            "window.__minicode_page_agent.injectSimulatorAura({})",
            status_json
        );
        cdp.evaluate_js(&script).await?;
        Ok(())
    }

    /// Updates the simulator aura status pill and glides cursor to target coordinates
    pub async fn update_simulator_action(
        cdp: &CdpClient,
        action_text: &str,
        x: f64,
        y: f64,
        is_click: bool,
    ) -> Result<()> {
        Self::inject_probe(cdp).await?;
        let action_json =
            serde_json::to_string(action_text).unwrap_or_else(|_| "\"Acting\"".to_string());
        let script = format!(
            "window.__minicode_page_agent.updateSimulatorState({}, {}, {}, {})",
            action_json, x, y, is_click
        );
        cdp.evaluate_js(&script).await?;
        Ok(())
    }

    /// Clears the visual simulator aura from the active page
    pub async fn clear_simulator_aura(cdp: &CdpClient) -> Result<bool> {
        Self::inject_probe(cdp).await?;
        let res = cdp
            .evaluate_js("window.__minicode_page_agent.clearSimulatorAura()")
            .await?;
        Ok(res.trim() == "true")
    }

    /// Clicks an element via DOM event dispatch, updates simulator aura, and returns coordinates
    pub async fn click_element_with_aura(
        cdp: &CdpClient,
        target_ref: &str,
        target_name: &str,
        selector: Option<&str>,
    ) -> Result<(f64, f64, String)> {
        Self::inject_probe(cdp).await?;
        let ref_json = serde_json::to_string(target_ref).unwrap_or_default();
        let name_json = serde_json::to_string(target_name).unwrap_or_default();
        let sel_json = match selector {
            Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };

        let script = format!(
            "window.__minicode_page_agent.clickElement({}, {}, {})",
            ref_json, name_json, sel_json
        );
        let res_json = cdp.evaluate_js(&script).await?;
        let parsed: serde_json::Value = serde_json::from_str(&res_json).map_err(|e| {
            ToolError::CommandExec(format!(
                "Failed parsing click result: {} -> {}",
                e, res_json
            ))
        })?;

        if parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let x = parsed.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = parsed.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let name = parsed
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(target_name)
                .to_string();
            Ok((x, y, name))
        } else {
            let err = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Click failed");
            Err(ToolError::CommandExec(err.to_string()).into())
        }
    }

    /// Fills text into an input/textarea/contenteditable element via PageAgent, updating simulator aura and React controlled state
    pub async fn fill_element_with_aura(
        cdp: &CdpClient,
        target_ref: &str,
        target_name: &str,
        selector: Option<&str>,
        text: &str,
    ) -> Result<(f64, f64, String)> {
        Self::inject_probe(cdp).await?;
        let ref_json = serde_json::to_string(target_ref).unwrap_or_default();
        let name_json = serde_json::to_string(target_name).unwrap_or_default();
        let sel_json = match selector {
            Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };
        let text_json = serde_json::to_string(text).unwrap_or_default();

        let script = format!(
            "window.__minicode_page_agent.inputText({}, {}, {}, {})",
            ref_json, name_json, sel_json, text_json
        );
        let res_json = cdp.evaluate_js(&script).await?;
        let parsed: serde_json::Value = serde_json::from_str(&res_json).map_err(|e| {
            ToolError::CommandExec(format!("Failed parsing fill result: {} -> {}", e, res_json))
        })?;

        if parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let x = parsed.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = parsed.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let name = parsed
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(target_name)
                .to_string();
            Ok((x, y, name))
        } else {
            let err = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Fill failed");
            Err(ToolError::CommandExec(err.to_string()).into())
        }
    }

    /// Selects an option in a `<select>` dropdown via PageAgent, updating simulator aura
    pub async fn select_option_with_aura(
        cdp: &CdpClient,
        target_ref: &str,
        target_name: &str,
        selector: Option<&str>,
        option_text: &str,
    ) -> Result<(f64, f64, String)> {
        Self::inject_probe(cdp).await?;
        let ref_json = serde_json::to_string(target_ref).unwrap_or_default();
        let name_json = serde_json::to_string(target_name).unwrap_or_default();
        let sel_json = match selector {
            Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };
        let opt_json = serde_json::to_string(option_text).unwrap_or_default();

        let script = format!(
            "window.__minicode_page_agent.selectOption({}, {}, {}, {})",
            ref_json, name_json, sel_json, opt_json
        );
        let res_json = cdp.evaluate_js(&script).await?;
        let parsed: serde_json::Value = serde_json::from_str(&res_json).map_err(|e| {
            ToolError::CommandExec(format!(
                "Failed parsing selectOption result: {} -> {}",
                e, res_json
            ))
        })?;

        if parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let x = parsed.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = parsed.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let selected = parsed
                .get("selected_text")
                .and_then(|v| v.as_str())
                .unwrap_or(option_text)
                .to_string();
            Ok((x, y, selected))
        } else {
            let err = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Select option failed");
            Err(ToolError::CommandExec(err.to_string()).into())
        }
    }

    /// Hovers over an element via PageAgent, updating simulator aura cursor and dispatching hover events
    pub async fn hover_element_with_aura(
        cdp: &CdpClient,
        target_ref: &str,
        target_name: &str,
        selector: Option<&str>,
    ) -> Result<(f64, f64, String)> {
        Self::inject_probe(cdp).await?;
        let ref_json = serde_json::to_string(target_ref).unwrap_or_default();
        let name_json = serde_json::to_string(target_name).unwrap_or_default();
        let sel_json = match selector {
            Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };

        let script = format!(
            "window.__minicode_page_agent.hoverElement({}, {}, {})",
            ref_json, name_json, sel_json
        );
        let res_json = cdp.evaluate_js(&script).await?;
        let parsed: serde_json::Value = serde_json::from_str(&res_json).map_err(|e| {
            ToolError::CommandExec(format!(
                "Failed parsing hover result: {} -> {}",
                e, res_json
            ))
        })?;

        if parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let x = parsed.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = parsed.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let name = parsed
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(target_name)
                .to_string();
            Ok((x, y, name))
        } else {
            let err = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Hover failed");
            Err(ToolError::CommandExec(err.to_string()).into())
        }
    }

    /// Scrolls the page with container fallback and updates simulator aura
    pub async fn scroll_with_container_fallback(
        cdp: &CdpClient,
        direction: &str,
        pixels: Option<i32>,
    ) -> Result<()> {
        Self::inject_probe(cdp).await?;
        let dir_json = serde_json::to_string(direction).unwrap_or_default();
        let px_json = match pixels {
            Some(px) => px.to_string(),
            None => "null".to_string(),
        };

        let script = format!(
            "window.__minicode_page_agent.scrollPage({}, {})",
            dir_json, px_json
        );
        cdp.evaluate_js(&script).await?;
        Ok(())
    }

    /// Presses a keyboard key (or shortcut with modifier mask) via PageAgent + native CDP key dispatch
    pub async fn press_key_with_aura(
        cdp: &CdpClient,
        key: &str,
        target_ref: Option<&str>,
        selector: Option<&str>,
        modifiers: u32,
    ) -> Result<String> {
        Self::inject_probe(cdp).await?;
        let key_json = serde_json::to_string(key).unwrap_or_else(|_| "\"Enter\"".to_string());
        let ref_json = match target_ref {
            Some(r) => serde_json::to_string(r).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };
        let sel_json = match selector {
            Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };
        let script = format!(
            "window.__minicode_page_agent.pressKey({key_json}, {ref_json}, '', {sel_json}, {modifiers})"
        );
        let res_json = cdp.evaluate_js(&script).await?;
        let parsed: serde_json::Value = serde_json::from_str(&res_json).unwrap_or_default();
        if parsed.get("ok").and_then(|v| v.as_bool()) == Some(false) {
            let err = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Key press failed");
            return Err(ToolError::CommandExec(err.to_string()).into());
        }
        let _ = cdp.dispatch_key_press(key, modifiers).await;
        Ok(parsed
            .get("tag")
            .and_then(|v| v.as_str())
            .unwrap_or("body")
            .to_string())
    }

    /// Toggles a checkbox, radio button, or ARIA switch deterministically to `checked`
    pub async fn set_checked_with_aura(
        cdp: &CdpClient,
        target_ref: Option<&str>,
        selector: Option<&str>,
        checked: bool,
    ) -> Result<bool> {
        Self::inject_probe(cdp).await?;
        let ref_json = serde_json::to_string(target_ref.unwrap_or("")).unwrap_or_default();
        let sel_json = match selector {
            Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };
        let script = format!(
            "window.__minicode_page_agent.setChecked({ref_json}, '', {sel_json}, {checked})"
        );
        let res_json = cdp.evaluate_js(&script).await?;
        let parsed: serde_json::Value = serde_json::from_str(&res_json).map_err(|e| {
            ToolError::CommandExec(format!("Failed parsing setChecked result: {e}"))
        })?;
        if parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            Ok(parsed
                .get("checked")
                .and_then(|v| v.as_bool())
                .unwrap_or(checked))
        } else {
            let err = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("setChecked failed");
            Err(ToolError::CommandExec(err.to_string()).into())
        }
    }

    /// Executes HTML5 DragEvent + PointerEvent drag-and-drop between two targets or by `(dx, dy)` offset
    pub async fn drag_and_drop_with_aura(
        cdp: &CdpClient,
        source_ref: Option<&str>,
        source_selector: Option<&str>,
        target_ref: Option<&str>,
        target_selector: Option<&str>,
        dx: Option<i32>,
        dy: Option<i32>,
    ) -> Result<((f64, f64), (f64, f64))> {
        Self::inject_probe(cdp).await?;
        let sref = serde_json::to_string(source_ref.unwrap_or("")).unwrap_or_default();
        let ssel = match source_selector {
            Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };
        let dref = serde_json::to_string(target_ref.unwrap_or("")).unwrap_or_default();
        let dsel = match target_selector {
            Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };
        let dx_js = dx.map(|v| v.to_string()).unwrap_or_else(|| "0".to_string());
        let dy_js = dy.map(|v| v.to_string()).unwrap_or_else(|| "0".to_string());
        let script = format!(
            "window.__minicode_page_agent.dragAndDrop({sref}, {ssel}, {dref}, {dsel}, {dx_js}, {dy_js})"
        );
        let res_json = cdp.evaluate_js(&script).await?;
        let parsed: serde_json::Value = serde_json::from_str(&res_json)
            .map_err(|e| ToolError::CommandExec(format!("Failed parsing drag result: {e}")))?;
        if parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let fx = parsed
                .pointer("/from/0")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let fy = parsed
                .pointer("/from/1")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let tx = parsed
                .pointer("/to/0")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let ty = parsed
                .pointer("/to/1")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            Ok(((fx, fy), (tx, ty)))
        } else {
            let err = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Drag and drop failed");
            Err(ToolError::CommandExec(err.to_string()).into())
        }
    }

    /// Dispatches `double_click` or `right_click` on an element via PageAgent
    pub async fn pointer_action_with_aura(
        cdp: &CdpClient,
        mode: &str,
        target_ref: Option<&str>,
        selector: Option<&str>,
    ) -> Result<(f64, f64)> {
        Self::inject_probe(cdp).await?;
        let mode_json =
            serde_json::to_string(mode).unwrap_or_else(|_| "\"double_click\"".to_string());
        let ref_json = serde_json::to_string(target_ref.unwrap_or("")).unwrap_or_default();
        let sel_json = match selector {
            Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };
        let script = format!(
            "window.__minicode_page_agent.pointerAction({mode_json}, {ref_json}, '', {sel_json})"
        );
        let res_json = cdp.evaluate_js(&script).await?;
        let parsed: serde_json::Value = serde_json::from_str(&res_json).map_err(|e| {
            ToolError::CommandExec(format!("Failed parsing pointerAction result: {e}"))
        })?;
        if parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let x = parsed.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = parsed.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            Ok((x, y))
        } else {
            let err = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Pointer action failed");
            Err(ToolError::CommandExec(err.to_string()).into())
        }
    }

    /// Synchronizes `window.__minicode_rev` with `revision` and scans the DOM tree including Shadow DOM
    pub async fn scan_visual_tree_at_revision(
        cdp: &CdpClient,
        revision: u32,
    ) -> Result<Vec<VisualElement>> {
        Self::inject_probe(cdp).await?;
        let script = format!(
            "(function() {{ window.__minicode_rev = {}; return JSON.stringify(window.__minicode_page_agent.scanInteractables()); }})()",
            revision.max(1)
        );
        let res_json = cdp.evaluate_js(&script).await?;
        let elements: Vec<VisualElement> = serde_json::from_str(&res_json).map_err(|e| {
            ToolError::CommandExec(format!("Failed parsing in-page visual elements: {}", e))
        })?;
        Ok(elements)
    }

    /// Scans the DOM tree including Shadow DOM and returns visible interactive elements with bounding boxes
    pub async fn scan_visual_tree(cdp: &CdpClient) -> Result<Vec<VisualElement>> {
        Self::inject_probe(cdp).await?;

        let res_json = cdp
            .evaluate_js("JSON.stringify(window.__minicode_page_agent.scanInteractables())")
            .await?;

        let elements: Vec<VisualElement> = serde_json::from_str(&res_json).map_err(|e| {
            ToolError::CommandExec(format!("Failed parsing in-page visual elements: {}", e))
        })?;

        Ok(elements)
    }

    /// Runs a comprehensive QA audit on the active page
    pub async fn run_qa_audit(cdp: &CdpClient, url: &str) -> Result<QaAuditReport> {
        Self::inject_probe(cdp).await?;

        let audit_json = cdp
            .evaluate_js("JSON.stringify(window.__minicode_page_agent.auditPage())")
            .await?;

        let raw_audit: RawPageAudit = serde_json::from_str(&audit_json).unwrap_or_default();
        let interactables = Self::scan_visual_tree(cdp).await.unwrap_or_default();

        let mut pass_count = 0;
        let mut warn_count = 0;
        let mut error_count = 0;
        let mut recommendations = Vec::new();

        // 1. Meta / SEO / Structure checks
        if raw_audit.has_viewport {
            pass_count += 1;
        } else {
            warn_count += 1;
            recommendations.push("Add a `<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">` tag for mobile responsiveness.".to_string());
        }

        if raw_audit.h1_count == 1 {
            pass_count += 1;
        } else if raw_audit.h1_count == 0 {
            warn_count += 1;
            recommendations.push("Page has no `<h1>` heading. Add a primary `<h1>` element for accessibility and semantic structure.".to_string());
        } else {
            warn_count += 1;
            recommendations.push(format!(
                "Page has multiple ({}) `<h1>` tags. Ensure only one top-level `<h1>` is used.",
                raw_audit.h1_count
            ));
        }

        // 2. Images check
        if raw_audit.broken_images.is_empty() {
            pass_count += 1;
        } else {
            error_count += raw_audit.broken_images.len();
            recommendations.push(format!(
                "Fix {} broken image(s) that failed to load or have empty `src` attributes.",
                raw_audit.broken_images.len()
            ));
        }

        // 3. Links check
        if raw_audit.broken_links.is_empty() {
            pass_count += 1;
        } else {
            warn_count += raw_audit.broken_links.len();
            recommendations.push(format!("Replace {} empty/placeholder link(s) (href=\"#\" or empty) with valid URLs or `<button>` elements.", raw_audit.broken_links.len()));
        }

        // 4. Runtime Console and Network errors
        let debug_report = cdp.debug_collector().format_report();
        let mut console_errors = Vec::new();
        let mut network_errors = Vec::new();

        for line in debug_report.lines() {
            if line.contains("[ERROR]") || line.contains("[exception]") {
                console_errors.push(line.to_string());
                error_count += 1;
            } else if line.contains("[HTTP 4") || line.contains("[HTTP 5") {
                network_errors.push(line.to_string());
                error_count += 1;
            }
        }

        if console_errors.is_empty() && network_errors.is_empty() {
            pass_count += 1;
        } else {
            if !console_errors.is_empty() {
                recommendations.push(format!(
                    "Resolve {} uncaught runtime JavaScript exception(s).",
                    console_errors.len()
                ));
            }
            if !network_errors.is_empty() {
                recommendations.push(format!(
                    "Fix {} failed HTTP network request(s) (4xx/5xx).",
                    network_errors.len()
                ));
            }
        }

        // 5. Taste-Skill v2.1 Layout & Typography warnings
        if raw_audit.layout_warnings.is_empty() {
            pass_count += 1;
        } else {
            warn_count += raw_audit.layout_warnings.len();
            for lw in raw_audit.layout_warnings {
                recommendations.push(lw);
            }
        }

        // 6. Fuse MiniBrowse Native Root-Cause Diagnostics (Shadow DOM overlays, CSS overflow culprits, WCAG contrast, occlusion)
        if let Ok(mb_diag) = cdp.minibrowse_web_diagnose().await {
            if let Some(overlays) = mb_diag.get("framework_overlays").and_then(|v| v.as_array()) {
                for ov in overlays {
                    let fw = ov
                        .get("framework")
                        .and_then(|v| v.as_str())
                        .unwrap_or("bundler");
                    let err = ov.get("error").and_then(|v| v.as_str()).unwrap_or("");
                    if !err.is_empty() {
                        error_count += 1;
                        console_errors.push(format!("[{fw}-overlay] {err}"));
                        recommendations.push(format!(
                            "Fix [{fw}] Shadow DOM compile/runtime error overlay: {err}"
                        ));
                    }
                }
            }
            if let Some(culprits) = mb_diag.get("overflow_culprits").and_then(|v| v.as_array()) {
                for c in culprits.iter().take(4) {
                    let sel = c
                        .get("selector")
                        .and_then(|v| v.as_str())
                        .unwrap_or("element");
                    let px = c.get("overflow_px").and_then(|v| v.as_i64()).unwrap_or(0);
                    let css = c.get("computed").cloned().unwrap_or_default();
                    recommendations.push(format!(
                        "[overflow-culprit] `{sel}` overflows viewport by +{px}px (computed: {css})"
                    ));
                }
            }
            if let Some(contrasts) = mb_diag.get("contrast_issues").and_then(|v| v.as_array()) {
                for ci in contrasts.iter().take(4) {
                    let sel = ci
                        .get("selector")
                        .and_then(|v| v.as_str())
                        .unwrap_or("element");
                    let ratio = ci
                        .get("contrast_ratio")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let req = ci
                        .get("required_ratio")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(4.5);
                    warn_count += 1;
                    recommendations.push(format!(
                        "[wcag-contrast] `{sel}` has contrast ratio {ratio:.2}:1 (minimum {req:.1}:1)"
                    ));
                }
            }
            if let Some(occ) = mb_diag
                .get("fixed_header_occlusion")
                .filter(|v| v.is_object())
            {
                let hdr = occ
                    .get("header_selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("header");
                let px = occ.get("overlap_px").and_then(|v| v.as_i64()).unwrap_or(0);
                if px > 0 {
                    warn_count += 1;
                    recommendations.push(format!(
                        "[header-occlusion] Fixed `{hdr}` overlaps `<h1>` by {px}px; add top padding to hero/main container."
                    ));
                }
            }
        }

        Ok(QaAuditReport {
            url: url.to_string(),
            title: raw_audit.title,
            pass_count,
            warn_count,
            error_count,
            broken_images: raw_audit.broken_images,
            broken_links: raw_audit.broken_links,
            console_errors,
            network_errors,
            forms_summary: raw_audit.forms_summary,
            interactive_count: interactables.len(),
            recommendations,
        })
    }

    /// Formats the QA audit into an agent-readable Markdown report
    pub fn format_qa_report(report: &QaAuditReport) -> String {
        let mut out = format!("### Website QA Audit Report: {}\n", report.title);
        out.push_str(&format!("**URL:** `{}`\n\n", report.url));

        let status_emoji = if report.error_count == 0 && report.warn_count == 0 {
            "🟢 **HEALTHY**"
        } else if report.error_count == 0 {
            "🟡 **WARNINGS FOUND**"
        } else {
            "🔴 **CRITICAL ISSUES DETECTED**"
        };

        out.push_str(&format!(
            "**Overall Status:** {} (✅ {} passed, ⚠️ {} warnings, ❌ {} errors)\n\n",
            status_emoji, report.pass_count, report.warn_count, report.error_count
        ));

        if !report.recommendations.is_empty() {
            out.push_str("#### Recommended Fixes:\n");
            for (i, rec) in report.recommendations.iter().enumerate() {
                out.push_str(&format!("{}. {}\n", i + 1, rec));
            }
            out.push('\n');
        }

        let push_section = |out: &mut String,
                            title: &str,
                            items: &[String],
                            strip_bullet: bool,
                            code_wrap: bool| {
            if !items.is_empty() {
                out.push_str(&format!("#### {}:\n", title));
                for item in items {
                    let clean = if strip_bullet {
                        item.trim().trim_start_matches('•').trim()
                    } else {
                        item.as_str()
                    };
                    if code_wrap {
                        out.push_str(&format!("• `{}`\n", clean));
                    } else {
                        out.push_str(&format!("• {}\n", clean));
                    }
                }
                out.push('\n');
            }
        };

        push_section(
            &mut out,
            "Runtime JavaScript Exceptions",
            &report.console_errors,
            true,
            false,
        );
        push_section(
            &mut out,
            "Network Loading Failures (4xx / 5xx)",
            &report.network_errors,
            true,
            false,
        );
        push_section(
            &mut out,
            "Broken Images",
            &report.broken_images,
            false,
            true,
        );
        push_section(
            &mut out,
            "Empty / Placeholder Links",
            &report.broken_links,
            false,
            false,
        );
        push_section(
            &mut out,
            "Forms & Inputs",
            &report.forms_summary,
            false,
            false,
        );

        out.push_str(&format!(
            "**Interactive Elements:** {} actionable components discovered on page.\n",
            report.interactive_count
        ));

        out
    }

    /// Formats in-page visual elements into a structured spatial Markdown table
    pub fn format_visual_tree_report(elements: &[VisualElement]) -> String {
        let mut out = format!(
            "### In-Page Visual DOM Tree ({} elements)\n\n",
            elements.len()
        );
        out.push_str("| Target Selector | Tag/Role | Text / Label | Coordinates (X, Y, W, H) | Shadow DOM |\n");
        out.push_str("| :--- | :--- | :--- | :--- | :--- |\n");

        for el in elements.iter().take(30) {
            let shadow_badge = if el.is_shadow_dom { "yes" } else { "no" };
            let clean_name = el.name.replace('|', "\\|").replace('\n', " ");
            let label = if clean_name.is_empty() {
                "_(no text)_"
            } else {
                &clean_name
            };
            out.push_str(&format!(
                "| `{}` | `<{}>` ({}) | {} | ({}, {}, {}x{}) | {} |\n",
                el.selector, el.tag, el.role, label, el.x, el.y, el.width, el.height, shadow_badge
            ));
        }

        if elements.len() > 30 {
            out.push_str(&format!(
                "\n_... +{} more visual elements omitted for token efficiency._\n",
                elements.len() - 30
            ));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_probe_js_syntax() {
        assert!(PAGE_PROBE_JS.starts_with("(function()"));
        assert!(PAGE_PROBE_JS.contains("scanInteractables"));
        assert!(PAGE_PROBE_JS.contains("resolveElement"));
        assert!(PAGE_PROBE_JS.contains("injectSimulatorAura"));
        assert!(PAGE_PROBE_JS.contains("minicode-simulator-container"));
        assert!(PAGE_PROBE_JS.contains("minicode-aura-left-rail"));
        assert!(PAGE_PROBE_JS.contains("minicode-aura-right-rail"));
        assert!(PAGE_PROBE_JS.contains("minicode-simulator-cursor"));
        assert!(PAGE_PROBE_JS.contains("minicode-simulator-pill"));
        assert!(PAGE_PROBE_JS.contains("clickElement"));
        assert!(PAGE_PROBE_JS.contains("scrollPage"));
        assert!(PAGE_PROBE_JS.contains("auditPage"));
        assert!(PAGE_PROBE_JS.ends_with("})()"));
    }

    #[test]
    fn test_format_qa_report() {
        let report = QaAuditReport {
            url: "http://localhost:3000".to_string(),
            title: "Test Store".to_string(),
            pass_count: 3,
            warn_count: 1,
            error_count: 1,
            broken_images: vec!["/logo-missing.png".to_string()],
            broken_links: vec!["\"Contact\" -> href=\"#\"".to_string()],
            console_errors: vec![
                "[ERROR] Uncaught TypeError: Cannot read property 'map'".to_string()
            ],
            network_errors: vec!["[HTTP 500] GET /api/cart".to_string()],
            forms_summary: vec!["Form #1 [POST /checkout]: 3 fields".to_string()],
            interactive_count: 12,
            recommendations: vec!["Fix broken image /logo-missing.png".to_string()],
        };

        let formatted = PageAgent::format_qa_report(&report);
        assert!(formatted.contains("CRITICAL ISSUES DETECTED"));
        assert!(formatted.contains("/logo-missing.png"));
        assert!(formatted.contains("Uncaught TypeError"));
        assert!(formatted.contains("[HTTP 500]"));
    }

    #[test]
    fn test_format_visual_tree_report() {
        let elements = vec![VisualElement {
            tag: "button".to_string(),
            role: "button".to_string(),
            name: "Add to Cart".to_string(),
            selector: "#add-to-cart-btn".to_string(),
            x: 100.0,
            y: 200.0,
            width: 150.0,
            height: 40.0,
            is_shadow_dom: false,
            is_scrollable: false,
            scroll_data: None,
            attributes: HashMap::new(),
        }];

        let formatted = PageAgent::format_visual_tree_report(&elements);
        assert!(formatted.contains("#add-to-cart-btn"));
        assert!(formatted.contains("Add to Cart"));
        assert!(formatted.contains("(100, 200, 150x40)"));
    }

    #[test]
    fn test_page_metrics_serialization() {
        let metrics = PageMetrics {
            viewport_width: 1920.0,
            viewport_height: 1080.0,
            page_width: 1920.0,
            page_height: 3240.0,
            scroll_x: 0.0,
            scroll_y: 1080.0,
            pixels_above: 1080.0,
            pixels_below: 1080.0,
            pages_above: 1.0,
            pages_below: 1.0,
            total_pages: 3.0,
            current_page_position: 0.5,
        };

        let json_str = serde_json::to_string(&metrics).unwrap();
        let parsed: PageMetrics = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.viewport_width, 1920.0);
        assert_eq!(parsed.pages_above, 1.0);
        assert_eq!(parsed.total_pages, 3.0);
    }

    #[test]
    fn test_page_probe_js_option_b_components() {
        assert!(PAGE_PROBE_JS.contains("minicode-aura-left-rail"));
        assert!(PAGE_PROBE_JS.contains("minicode-aura-right-rail"));
        assert!(PAGE_PROBE_JS.contains("minicode-equalizer"));
        assert!(PAGE_PROBE_JS.contains("minicode-equalizer-bar"));
        assert!(PAGE_PROBE_JS.contains("minicode-eq-1"));
        assert!(PAGE_PROBE_JS.contains("HALT ■"));
        assert!(PAGE_PROBE_JS.contains("--minicode-theme-color-rgb"));
        assert!(PAGE_PROBE_JS.contains("prefers-color-scheme: light"));
        assert!(PAGE_PROBE_JS.contains("inputText"));
        assert!(PAGE_PROBE_JS.contains("selectOption"));
        assert!(PAGE_PROBE_JS.contains("hoverElement"));
        assert!(PAGE_PROBE_JS.contains("__minicode_halted"));
        assert!(PAGE_PROBE_JS.contains("[MASKED]"));
    }

    #[test]
    fn test_scroll_data_and_page_metrics_formatting() {
        let sd = ScrollData {
            top: 50.0,
            bottom: 320.0,
            left: 0.0,
            right: 140.0,
        };
        assert_eq!(sd.format_compact().as_deref(), Some("↑50px ↓320px →140px"));

        let empty_sd = ScrollData {
            top: 0.0,
            bottom: 0.0,
            left: 0.0,
            right: 0.0,
        };
        assert!(empty_sd.format_compact().is_none());

        let metrics = PageMetrics {
            viewport_width: 1280.0,
            viewport_height: 720.0,
            page_width: 1280.0,
            page_height: 2160.0,
            scroll_x: 0.0,
            scroll_y: 720.0,
            pixels_above: 720.0,
            pixels_below: 720.0,
            pages_above: 1.0,
            pages_below: 1.0,
            total_pages: 3.0,
            current_page_position: 0.5,
        };
        let summary = metrics.format_summary();
        assert!(summary.contains("1280x720px"));
        assert!(summary.contains("1.00 pages above (720px)"));
        assert!(summary.contains("1.00 pages below (720px)"));
        assert!(summary.contains("Total Height: 3.00 pages (2160px)"));
    }

    #[test]
    fn test_page_probe_js_ref_priority_and_unique_selector_path() {
        let resolve_pos = PAGE_PROBE_JS
            .find("resolveElement: function(targetRef, targetName, selector)")
            .expect("resolveElement present");
        let resolve_slice = &PAGE_PROBE_JS[resolve_pos..];
        let ref_query_pos = resolve_slice
            .find("data-minicode-ref")
            .expect("stamped ref lookup present");
        let idx_query_pos = resolve_slice
            .find("data-minicode-idx")
            .expect("stamped idx lookup present");
        let sel_query_pos = resolve_slice
            .find("document.querySelector(selector)")
            .expect("selector fallback present");

        // Stamped live DOM ref and index MUST take precedence over selector_hint
        assert!(ref_query_pos < sel_query_pos);
        assert!(idx_query_pos < sel_query_pos);
        assert!(resolve_slice.contains("bySel && (clean.length === 0 || matchesTargetName(bySel))"));
        assert!(PAGE_PROBE_JS.contains("isUniqueSelector"));
        assert!(PAGE_PROBE_JS.contains("parts.join(' > ')"));
    }
}
