/**
 * minicode Landing Page — Interactive Scripts
 * Features:
 * 1. Live Theme Accent Switcher (Purple, Cyan, Mint, Coral)
 * 2. Interactive Terminal Streaming Simulator
 * 3. Tool Category Filter Tabs
 * 4. Install Command Switcher & One-Click Clipboard
 * 5. Interactive Swarm DAG Inspector
 */

document.addEventListener('DOMContentLoaded', () => {
    initThemePicker();
    initCopyButtons();
    initInstallTabs();
    initToolsFilter();
    initTerminalFeedSimulation();
    initDagInspector();
});

/**
 * 1. Live Theme Accent Switcher
 */
function initThemePicker() {
    const dots = document.querySelectorAll('.theme-dot');
    dots.forEach(dot => {
        dot.addEventListener('click', () => {
            dots.forEach(d => d.classList.remove('active'));
            dot.classList.add('active');

            const color = dot.getAttribute('data-color');
            document.body.className = '';
            document.body.classList.add(`theme-${color}`);
        });
    });
}

/**
 * 2. Copy Command Handlers with Feedback
 */
function initCopyButtons() {
    // Hero Quick Copy Command
    const heroCopy = document.getElementById('heroCopyCmd');
    if (heroCopy) {
        heroCopy.addEventListener('click', () => {
            const cmd = 'curl -fsSL https://minicode.dev/install.sh | bash';
            navigator.clipboard.writeText(cmd).then(() => {
                const icon = document.getElementById('copyIcon');
                if (icon) icon.textContent = '✔ Copied!';
                setTimeout(() => {
                    if (icon) icon.textContent = '📋';
                }, 2000);
            });
        });
    }

    // Install Box Copy Button
    const btnCopyInstall = document.getElementById('btnCopyInstall');
    const installCommandText = document.getElementById('installCommandText');
    if (btnCopyInstall && installCommandText) {
        btnCopyInstall.addEventListener('click', () => {
            navigator.clipboard.writeText(installCommandText.textContent.trim()).then(() => {
                const prev = btnCopyInstall.textContent;
                btnCopyInstall.textContent = 'Copied! ✔';
                setTimeout(() => {
                    btnCopyInstall.textContent = prev;
                }, 2000);
            });
        });
    }
}

/**
 * 3. Install Tabs Switcher
 */
function initInstallTabs() {
    const tabs = document.querySelectorAll('.inst-tab');
    const commandText = document.getElementById('installCommandText');
    if (!tabs.length || !commandText) return;

    const commands = {
        curl: 'curl -fsSL https://minicode.dev/install.sh | bash',
        cargo: 'cargo install minicode --locked',
        source: 'git clone https://github.com/aswin402/minicode.git && cd minicode && cargo build --release'
    };

    tabs.forEach(tab => {
        tab.addEventListener('click', () => {
            tabs.forEach(t => t.classList.remove('active'));
            tab.classList.add('active');

            const key = tab.getAttribute('data-inst');
            if (commands[key]) {
                commandText.textContent = commands[key];
            }
        });
    });
}

/**
 * 4. Tool Category Filter Tabs
 */
function initToolsFilter() {
    const tabs = document.querySelectorAll('.tool-tab');
    const cards = document.querySelectorAll('.t-card');

    tabs.forEach(tab => {
        tab.addEventListener('click', () => {
            tabs.forEach(t => t.classList.remove('active'));
            tab.classList.add('active');

            const cat = tab.getAttribute('data-cat');
            cards.forEach(card => {
                if (cat === 'all' || card.getAttribute('data-cat') === cat) {
                    card.style.display = 'block';
                } else {
                    card.style.display = 'none';
                }
            });
        });
    });
}

/**
 * 5. Interactive Terminal Streaming Simulator
 */
function initTerminalFeedSimulation() {
    const feed = document.getElementById('terminalFeed');
    if (!feed) return;

    // Pulse effect on terminal title chip
    setInterval(() => {
        const dot = document.querySelector('.status-live-dot');
        if (dot) {
            dot.style.opacity = dot.style.opacity === '0.4' ? '1' : '0.4';
        }
    }, 1000);
}

/**
 * 6. Interactive Swarm DAG Inspector
 */
function initDagInspector() {
    const dagCards = document.querySelectorAll('.dag-card');
    dagCards.forEach(card => {
        card.style.cursor = 'pointer';
        card.addEventListener('click', () => {
            dagCards.forEach(c => c.style.outline = 'none');
            card.style.outline = '2px solid var(--theme-accent)';
        });
    });
}
