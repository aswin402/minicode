use super::driver::CdpClient;
use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Embedded in-page probe that runs inside the browser DOM context.
/// Pierces Shadow DOM, computes accurate layout geometry, and checks accessibility.
pub const PAGE_PROBE_JS: &str = r###"(function() {
    if (window.__minicode_page_agent) return 'already_installed';

    window.__minicode_page_agent = {
        // Pierces light DOM and Shadow DOM to find visible interactive elements and stamps DOM refs
        scanInteractables: function() {
            var results = [];
            var idCounter = 1;
            var rev = window.__minicode_rev || 1;

            function getShortSelector(el) {
                if (el.id) return '#' + CSS.escape(el.id);
                if (el.getAttribute('data-testid')) return '[data-testid="' + CSS.escape(el.getAttribute('data-testid')) + '"]';
                if (el.getAttribute('name')) return el.tagName.toLowerCase() + '[name="' + CSS.escape(el.getAttribute('name')) + '"]';
                if (el.getAttribute('aria-label')) return el.tagName.toLowerCase() + '[aria-label="' + CSS.escape(el.getAttribute('aria-label')) + '"]';
                
                var tag = el.tagName.toLowerCase();
                var parent = el.parentElement;
                if (!parent) return tag;
                var siblings = Array.from(parent.children).filter(function(c) { return c.tagName === el.tagName; });
                if (siblings.length === 1) return tag;
                var index = siblings.indexOf(el) + 1;
                return tag + ':nth-of-type(' + index + ')';
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
                var hasScrollY = /(auto|scroll|overlay)/.test(style.overflowY) && el.scrollHeight > el.clientHeight;
                var hasScrollX = /(auto|scroll|overlay)/.test(style.overflowX) && el.scrollWidth > el.clientWidth;
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
                    var isNativeInteractive = /^(button|a|input|select|textarea|summary)$/.test(tag);
                    var isAriaInteractive = /^(button|link|checkbox|radio|combobox|menuitem|tab|switch)$/.test(role);
                    var hasClick = el.onclick !== null || el.hasAttribute('onclick');
                    var isFocusable = el.hasAttribute('tabindex') && el.getAttribute('tabindex') !== '-1';

                    if (isNativeInteractive || isAriaInteractive || hasClick || isFocusable) {
                        var rect = el.getBoundingClientRect();
                        var style = window.getComputedStyle(el);

                        if (isElementVisible(el, rect, style)) {
                            var text = (el.innerText || el.value || el.getAttribute('placeholder') || el.getAttribute('aria-label') || '').trim();
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
                            if (el.getAttribute('placeholder')) attrs.placeholder = el.getAttribute('placeholder');
                            if (el.getAttribute('href')) attrs.href = el.getAttribute('href');
                            if (el.getAttribute('data-testid')) attrs.data_testid = el.getAttribute('data-testid');

                            var scrollData = checkScrollable(el, style);

                            results.push({
                                tag: tag,
                                role: role,
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

        // Resolves target DOM node by ref (@v1:e7), numeric index, ID, attribute, or text content
        resolveElement: function(targetRef, targetName, selector) {
            if (selector) {
                var el = document.querySelector(selector);
                if (el) return el;
            }
            if (targetRef) {
                var clean = targetRef.trim();
                // 1. Exact match on stamped data-minicode-ref
                var byRef = document.querySelector('[data-minicode-ref="' + CSS.escape(clean) + '"]');
                if (byRef) return byRef;

                // 2. Numeric suffix match on data-minicode-idx (e.g. '@v1:e7' -> '7' or '7')
                var match = clean.match(/e?(\d+)$/);
                if (match) {
                    var byIdx = document.querySelector('[data-minicode-idx="' + match[1] + '"]');
                    if (byIdx) return byIdx;
                }

                // 3. ID match
                var idClean = clean.startsWith('#') ? clean.slice(1) : clean;
                var byId = document.getElementById(idClean);
                if (byId) return byId;

                // 4. Exact attribute match
                try {
                    var byAttr = document.querySelector('[name="' + CSS.escape(clean) + '"], [data-testid="' + CSS.escape(clean) + '"], [aria-label="' + CSS.escape(clean) + '"]');
                    if (byAttr) return byAttr;
                } catch (_) {}
            }

            // 5. Intelligent fuzzy match by targetName among interactive elements
            if (targetName && targetName.trim().length > 0) {
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

            // 6. Pierce Shadow DOMs for matching target
            function searchShadow(root) {
                var els = root.querySelectorAll ? Array.from(root.querySelectorAll('*')) : [];
                for (var i = 0; i < els.length; i++) {
                    var e = els[i];
                    if (targetRef && (e.getAttribute('data-minicode-ref') === targetRef || e.id === targetRef || e.getAttribute('data-testid') === targetRef)) return e;
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
                    <span id="minicode-pill-text">${statusText ? statusText.replace(/</g, '&lt;') : 'AI Agent Active'}</span>
                    <span id="minicode-pill-stop" title="Halt Agent">HALT ■</span>
                </div>
            `;
            document.body.appendChild(container);
            return true;
        },

        // Updates simulator state: updates pill message, glides cursor, triggers ripple, activates rail wave & equalizer
        updateSimulatorState: function(actionText, targetX, targetY, isClick, themeColor) {
            this.injectSimulatorAura(actionText, themeColor);

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
            var c = document.getElementById('minicode-simulator-container');
            if (c) c.remove();
            var s = document.getElementById('minicode-simulator-styles');
            if (s) s.remove();
            return true;
        },

        // Robustly clicks an element: scrolls into view, updates aura, and dispatches full W3C sequence
        clickElement: function(targetRef, targetName, selector) {
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

        // Scrolls viewport with fallback to largest scrollable container in DOM
        scrollPage: function(direction, pixels) {
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
                            if (/(auto|scroll|overlay)/.test(style.overflowY)) {
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

            return {
                title: title,
                has_viewport: hasViewport,
                h1_count: h1Count,
                broken_images: brokenImages,
                broken_links: brokenLinks,
                forms_summary: formsSummary,
                total_images: images.length,
                total_links: links.length
            };
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
    /// Injects the probe into the active page context
    pub async fn inject_probe(cdp: &CdpClient) -> Result<()> {
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
    #[allow(dead_code)]
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

        if !report.console_errors.is_empty() {
            out.push_str("#### Runtime JavaScript Exceptions:\n");
            for err in &report.console_errors {
                out.push_str(&format!("• {}\n", err));
            }
            out.push('\n');
        }

        if !report.network_errors.is_empty() {
            out.push_str("#### Network Loading Failures (4xx / 5xx):\n");
            for err in &report.network_errors {
                out.push_str(&format!("• {}\n", err));
            }
            out.push('\n');
        }

        if !report.broken_images.is_empty() {
            out.push_str("#### Broken Images:\n");
            for img in &report.broken_images {
                out.push_str(&format!("• `{}`\n", img));
            }
            out.push('\n');
        }

        if !report.broken_links.is_empty() {
            out.push_str("#### Empty / Placeholder Links:\n");
            for link in &report.broken_links {
                out.push_str(&format!("• {}\n", link));
            }
            out.push('\n');
        }

        if !report.forms_summary.is_empty() {
            out.push_str("#### Forms & Inputs:\n");
            for f in &report.forms_summary {
                out.push_str(&format!("• {}\n", f));
            }
            out.push('\n');
        }

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
    }
}
