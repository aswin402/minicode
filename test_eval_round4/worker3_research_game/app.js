/**
 * QuestDo — Main Application
 * UI rendering, event handling, and game flow
 */

// ── Theme Config ──────────────────────────────────────────────────────────
const THEME = {
  bg:        '#0d0d14',
  surface:   '#16161f',
  card:      '#1e1e2e',
  border:    '#2a2a40',
  primary:   '#f7b731',
  accent:    '#a29bfe',
  danger:    '#eb3b5a',
  success:   '#20bf6b',
  text:      '#e8e8f0',
  muted:     '#6b6b8a',
  streak:    '#fd79a8'
};

// ── DOM Refs ──────────────────────────────────────────────────────────────
let $app, $statsPanel, $taskList, $xpBar, $streakDisplay;

// ── Init ─────────────────────────────────────────────────────────────────
const initApp = () => {
  injectStyles();
  EffectsEngine.initConfetti();

  $app = document.getElementById('app');

  GameState.loadAll();
  const player = GameState.getPlayer();

  if (player.name === 'Hero' && !localStorage.getItem('questdo_player')) {
    renderOnboarding();
  } else {
    // Check for new day
    const lastDate = localStorage.getItem('questdo_lastDate');
    const today = new Date().toISOString().slice(0, 10);
    if (lastDate && lastDate !== today) {
      GameState.updateStreak();
    }
    localStorage.setItem('questdo_lastDate', today);
    renderGame();
  }

  GameState.onUpdate(renderStats);
};

// ── Onboarding ────────────────────────────────────────────────────────────
const renderOnboarding = () => {
  const classes = GameState.getClassData();

  $app.innerHTML = `
    <div class="onboarding">
      <div class="logo-title">⚔️ QuestDo</div>
      <p class="logo-sub">Turn your tasks into legendary quests</p>

      <div class="name-field">
        <label for="hero-name">🏅 Enter Your Name</label>
        <input id="hero-name" type="text" placeholder="Your hero name..." maxlength="20" autocomplete="off" />
      </div>

      <div class="class-grid" id="class-grid">
        ${Object.entries(classes).map(([key, cls]) => `
          <div class="class-card" data-class="${key}" style="--cls-color:${cls.color}">
            <div class="class-icon">${cls.icon}</div>
            <div class="class-name">${cls.name}</div>
            <div class="class-desc">${cls.desc}</div>
          </div>
        `).join('')}
      </div>

      <button id="start-btn" class="start-btn" disabled>⚔️ Begin Your Quest</button>
    </div>
  `;

  let selectedClass = null;
  const nameInput = document.getElementById('hero-name');
  const startBtn  = document.getElementById('start-btn');
  const classCards = document.querySelectorAll('.class-card');

  nameInput.addEventListener('input', () => {
    startBtn.disabled = !selectedClass || !nameInput.value.trim();
  });

  classCards.forEach(card => {
    card.addEventListener('click', () => {
      classCards.forEach(c => c.classList.remove('selected'));
      card.classList.add('selected');
      selectedClass = card.dataset.class;
      startBtn.disabled = !nameInput.value.trim();
      AudioEngine.playComplete();
    });
  });

  startBtn.addEventListener('click', () => {
    if (!selectedClass || !nameInput.value.trim()) return;
    GameState.initPlayer(nameInput.value.trim(), selectedClass);
    EffectsEngine.burstScreenCenter(80);
    AudioEngine.playLevelUp();
    setTimeout(() => renderGame(), 300);
  });

  nameInput.focus();
};

// ── Main Game Screen ──────────────────────────────────────────────────────
const renderGame = () => {
  $app.innerHTML = `
    <div class="game-layout">
      <!-- Stats Panel -->
      <div class="stats-panel" id="stats-panel">
        <div class="avatar-row">
          <div class="avatar" id="char-icon">${GameState.getClassData()[GameState.getPlayer().class]?.icon || '⚔️'}</div>
          <div class="char-info">
            <div class="char-name" id="char-name">${GameState.getPlayer().name}</div>
            <div class="char-class" id="char-class">${GameState.getPlayer().class}</div>
            <div class="char-title" id="char-title">Apprentice</div>
          </div>
          <div class="level-badge level-badge" id="level-badge">Lv <span id="level-num">${GameState.getPlayer().level}</span></div>
        </div>

        <!-- XP Bar -->
        <div class="xp-section">
          <div class="xp-label">
            <span>Experience</span>
            <span id="xp-text">0 / 50 XP</span>
          </div>
          <div class="xp-track">
            <div class="xp-bar-fill" id="xp-fill" style="width:0%"></div>
          </div>
        </div>

        <!-- Streak -->
        <div class="streak-section" id="streak-section">
          <div class="streak-row">
            <span class="streak-label" id="streak-display">🔥 <span id="streak-num">0</span>-day streak</span>
            <span class="multiplier-badge" id="mult-badge">×1.0</span>
          </div>
        </div>

        <!-- Stats Row -->
        <div class="stats-row">
          <div class="stat-item">
            <div class="stat-val" id="stat-completed">${GameState.getPlayer().totalTasksCompleted}</div>
            <div class="stat-key">Completed</div>
          </div>
          <div class="stat-item">
            <div class="stat-val" id="stat-longest">${GameState.getPlayer().longestStreak}</div>
            <div class="stat-key">Best Streak</div>
          </div>
          <div class="stat-item">
            <div class="stat-val" id="stat-perks">${GameState.getPlayer().perks.length}</div>
            <div class="stat-key">Perks</div>
          </div>
        </div>

        <!-- Perks -->
        <div class="perks-section" id="perks-section">
          <div class="perks-title">✨ Active Perks</div>
          <div class="perks-list" id="perks-list"></div>
        </div>

        <!-- Settings -->
        <div class="settings-row">
          <button class="icon-btn" id="mute-btn" title="Toggle Sound">🔊</button>
          <button class="icon-btn" id="export-btn" title="Export State">📤</button>
          <button class="icon-btn" id="import-btn" title="Import State">📥</button>
          <button class="icon-btn" id="reset-btn" title="Reset Game">🗑️</button>
        </div>
      </div>

      <!-- Quest Panel -->
      <div class="quest-panel">
        <h2 class="panel-title">📜 Active Quests</h2>

        <!-- Add Task Form -->
        <form class="add-task-form" id="add-task-form">
          <input id="task-input" type="text" placeholder="New quest description..." maxlength="80" autocomplete="off" />
          <select id="task-difficulty">
            <option value="easy">🟢 Easy (+10)</option>
            <option value="medium" selected>🟡 Medium (+20)</option>
            <option value="hard">🟠 Hard (+30)</option>
            <option value="epic">🔴 Epic (+50)</option>
          </select>
          <button type="submit" class="add-btn">+ Add Quest</button>
        </form>

        <!-- Task List -->
        <div class="task-list" id="task-list"></div>

        <!-- Empty State -->
        <div class="empty-state" id="empty-state">
          <div class="empty-icon">🗺️</div>
          <div class="empty-text">No active quests yet.</div>
          <div class="empty-sub">Add your first task above to begin!</div>
        </div>
      </div>
    </div>

    <!-- Hidden file input for import -->
    <input type="file" id="import-file" accept=".json" style="display:none" />
  `;

  bindEvents();
  renderStats();
  renderTasks();
  renderPerks();
};

// ── Event Binding ─────────────────────────────────────────────────────────
const bindEvents = () => {
  document.getElementById('add-task-form').addEventListener('submit', handleAddTask);
  document.getElementById('mute-btn').addEventListener('click', toggleMute);
  document.getElementById('export-btn').addEventListener('click', handleExport);
  document.getElementById('import-btn').addEventListener('click', () => document.getElementById('import-file').click());
  document.getElementById('import-file').addEventListener('change', handleImport);
  document.getElementById('reset-btn').addEventListener('click', handleReset);

  // Keyboard shortcut: press Enter on task input = quick add
  document.getElementById('task-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      document.getElementById('add-task-form').dispatchEvent(new Event('submit'));
    }
  });
};

const handleAddTask = (e) => {
  e.preventDefault();
  const input = document.getElementById('task-input');
  const diff  = document.getElementById('task-difficulty').value;
  const title = input.value.trim();

  if (!title) { AudioEngine.playError(); return; }

  GameState.addTask(title, diff);
  AudioEngine.playComplete();
  input.value = '';
  renderTasks();
};

const handleCompleteTask = (id, el) => {
  const rect = el.getBoundingClientRect();
  const result = GameState.completeTask(id);

  AudioEngine.playComplete();
  if (result.mult > 1) AudioEngine.playStreakBonus();

  // Float XP text
  EffectsEngine.showXPFloat(result.earned, rect.left + rect.width / 2, rect.top);

  // Confetti burst
  EffectsEngine.burst(rect.left + rect.width / 2, rect.top, 50);

  // Level up?
  if (result.leveled > 0 && result.newPerk) {
    AudioEngine.playLevelUp();
    EffectsEngine.showLevelUp(GameState.getPlayer().level, result.newPerk);

    // Animate level badge
    const badge = document.getElementById('level-badge');
    if (badge) {
      badge.classList.add('leveling');
      setTimeout(() => badge.classList.remove('leveling'), 1000);
    }
  }

  // Animate XP bar
  const fill = document.getElementById('xp-fill');
  if (fill) {
    fill.classList.add('animating');
    setTimeout(() => fill.classList.remove('animating'), 1200);
  }

  renderStats();
  renderTasks();
  renderPerks();
};

const handleDeleteTask = (id) => {
  GameState.deleteTask(id);
  AudioEngine.playError();
  renderTasks();
};

const toggleMute = () => {
  const btn = document.getElementById('mute-btn');
  const muted = !AudioEngine.isMuted();
  AudioEngine.setMuted(muted);
  btn.textContent = muted ? '🔇' : '🔊';
};

const handleExport = () => {
  const json = GameState.exportState();
  const blob = new Blob([json], { type: 'application/json' });
  const url  = URL.createObjectURL(blob);
  const a    = document.createElement('a');
  a.href = url; a.download = 'questdo-save.json'; a.click();
  URL.revokeObjectURL(url);
};

const handleImport = (e) => {
  const file = e.target.files[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = (ev) => {
    GameState.importState(ev.target.result);
    AudioEngine.playLevelUp();
    renderGame();
  };
  reader.readAsText(file);
  e.target.value = '';
};

const handleReset = () => {
  if (confirm('⚠️ Reset all progress? This cannot be undone.')) {
    GameState.resetGame();
    AudioEngine.playDailyReset();
    renderOnboarding();
  }
};

// ── Render Stats ───────────────────────────────────────────────────────────
const renderStats = () => {
  const player = GameState.getPlayer();
  const clsData = GameState.getClassData()[player.class];

  const el = id => document.getElementById(id);

  if (el('char-icon'))  el('char-icon').textContent  = clsData?.icon || '⚔️';
  if (el('char-name'))  el('char-name').textContent  = player.name;
  if (el('char-class')) el('char-class').textContent  = clsData?.name || player.class;
  if (el('char-title')) el('char-title').textContent  = GameState.getTitle();
  if (el('level-num'))  el('level-num').textContent   = player.level;

  const xpCurr = GameState.currentLevelXP();
  const xpNeeded = GameState.xpForLevel(player.level + 1);
  if (el('xp-text')) el('xp-text').textContent = `${xpCurr} / ${xpNeeded} XP`;
  if (el('xp-fill')) el('xp-fill').style.width  = `${GameState.xpPercent()}%`;

  const streak = GameState.getStreakCount();
  const mult   = GameState.getStreakMultiplier(streak);
  if (el('streak-num')) el('streak-num').textContent = streak;
  if (el('mult-badge')) {
    el('mult-badge').textContent = `×${mult % 1 === 0 ? mult : mult.toFixed(1)}`;
    el('mult-badge').className = 'multiplier-badge';
    if (streak >= 7) {
      el('streak-display').classList.add('streak-fire');
    }
  }

  if (el('stat-completed')) el('stat-completed').textContent = player.totalTasksCompleted;
  if (el('stat-longest'))   el('stat-longest').textContent   = player.longestStreak;
  if (el('stat-perks'))     el('stat-perks').textContent     = player.perks.length;
};

// ── Render Tasks ───────────────────────────────────────────────────────────
const DIFF_ICONS = { easy: '🟢', medium: '🟡', hard: '🟠', epic: '🔴' };

const renderTasks = () => {
  const list  = document.getElementById('task-list');
  const empty = document.getElementById('empty-state');
  if (!list) return;

  const tasks = GameState.getTasks();
  const pending = tasks.filter(t => !t.completed);

  empty.style.display = pending.length === 0 ? 'flex' : 'none';

  // Only re-render if DOM structure might have changed count
  // Use efficient update: diff against existing items
  const existing = Array.from(list.querySelectorAll('.task-card[data-id]'));
  const existingIds = new Set(existing.map(el => el.dataset.id));
  const pendingIds  = new Set(pending.map(t => t.id));

  // Remove deleted
  existing.forEach(el => {
    if (!pendingIds.has(el.dataset.id)) el.remove();
  });

  // Add new
  pending.forEach(task => {
    if (!existingIds.has(task.id)) {
      const card = createTaskCard(task);
      list.appendChild(card);
    }
  });
};

const createTaskCard = (task) => {
  const card = document.createElement('div');
  card.className = 'task-card';
  card.dataset.id = task.id;

  card.innerHTML = `
    <button class="task-check" aria-label="Complete quest" data-action="complete">
      <span class="check-box">○</span>
    </button>
    <div class="task-info">
      <div class="task-title">${escapeHtml(task.title)}</div>
      <div class="task-meta">
        <span class="diff-badge diff-${task.difficulty}">${DIFF_ICONS[task.difficulty]} ${task.difficulty}</span>
        <span class="xp-chip">+${task.xp} XP</span>
        ${task.streak > 1 ? `<span class="streak-chip">🔥 ${task.streak}</span>` : ''}
      </div>
    </div>
    <button class="task-delete" aria-label="Delete quest" data-action="delete">✕</button>
  `;

  card.querySelector('[data-action="complete"]').addEventListener('click', (e) => {
    card.classList.add('completing');
    handleCompleteTask(task.id, card);
  });

  card.querySelector('[data-action="delete"]').addEventListener('click', () => {
    card.style.animation = 'taskCompletePulse .2s ease-out reverse';
    setTimeout(() => handleDeleteTask(task.id), 200);
  });

  return card;
};

// ── Render Perks ────────────────────────────────────────────────────────────
const renderPerks = () => {
  const list = document.getElementById('perks-list');
  if (!list) return;

  const perks = GameState.getPerks();
  const active = perks.filter(p => p.active);

  if (active.length === 0) {
    list.innerHTML = '<div class="perks-empty">No perks yet — keep leveling!</div>';
    return;
  }

  list.innerHTML = active.map(p => `
    <div class="perk-chip">
      <span class="perk-name">${p.name}</span>
      <span class="perk-desc">${p.desc}</span>
    </div>
  `).join('');
};

// ── Utility ────────────────────────────────────────────────────────────────
const escapeHtml = (str) => {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
};

// ── Styles ─────────────────────────────────────────────────────────────────
const injectStyles = () => {
  if (document.getElementById('questdo-styles')) return;
  const css = `
    /* Reset & Root */
    *, *::before, *::after { box-sizing:border-box; margin:0; padding:0; }

    :root {
      --bg:       #0d0d14;
      --surface:  #16161f;
      --card:     #1e1e2e;
      --border:   #2a2a40;
      --primary:  #f7b731;
      --accent:   #a29bfe;
      --danger:   #eb3b5a;
      --success:  #20bf6b;
      --text:     #e8e8f0;
      --muted:    #6b6b8a;
      --streak:   #fd79a8;
    }

    html, body {
      font-family:'Segoe UI', system-ui, -apple-system, sans-serif;
      background: var(--bg);
      color: var(--text);
      min-height:100vh;
      line-height:1.5;
    }

    #app {
      max-width:660px;
      margin:0 auto;
      padding:1rem 1rem 4rem;
    }

    /* ── Onboarding ── */
    .onboarding {
      text-align:center;
      padding-top:3rem;
    }
    .logo-title {
      font-size:3rem;font-weight:900;color:var(--primary);
      text-shadow:0 0 30px rgba(247,183,49,.5);
      margin-bottom:.3rem;
    }
    .logo-sub { color:var(--muted);font-size:1.1rem;margin-bottom:2.5rem; }

    .name-field {
      margin-bottom:2rem;
    }
    .name-field label {
      display:block;font-size:.85rem;color:var(--accent);margin-bottom:.4rem;letter-spacing:1px;
    }
    .name-field input {
      width:100%;max-width:360px;padding:.8rem 1.2rem;font-size:1rem;
      background:var(--card);border:2px solid var(--border);border-radius:12px;
      color:var(--text);outline:none;transition:border-color .2s;
    }
    .name-field input:focus { border-color:var(--accent); }

    .class-grid {
      display:grid;grid-template-columns:repeat(3,1fr);gap:1rem;
      margin-bottom:2rem;
    }
    .class-card {
      background:var(--card);border:2px solid var(--border);border-radius:16px;
      padding:1.2rem .8rem;cursor:pointer;transition:all .2s;
      border-top:4px solid var(--cls-color,var(--accent));
    }
    .class-card:hover { transform:translateY(-3px);box-shadow:0 8px 24px rgba(0,0,0,.4); }
    .class-card.selected {
      border-color:var(--cls-color,var(--accent));
      background:color-mix(in srgb,var(--cls-color,var(--accent)) 15%,var(--card));
      transform:translateY(-3px);box-shadow:0 0 20px color-mix(in srgb,var(--cls-color) 40%,transparent);
    }
    .class-icon { font-size:2.4rem;margin-bottom:.5rem; }
    .class-name { font-weight:700;font-size:1rem;color:var(--text);margin-bottom:.4rem; }
    .class-desc { font-size:.72rem;color:var(--muted);line-height:1.4; }

    .start-btn {
      padding:1rem 3rem;font-size:1.1rem;font-weight:700;
      background:linear-gradient(135deg,var(--primary),#e67e22);
      color:#0d0d14;border:none;border-radius:50px;cursor:pointer;
      box-shadow:0 4px 20px rgba(247,183,49,.35);
      transition:all .2s;
    }
    .start-btn:disabled { opacity:.4;cursor:not-allowed;transform:none; }
    .start-btn:not(:disabled):hover { transform:scale(1.06);box-shadow:0 6px 28px rgba(247,183,49,.55); }

    /* ── Game Layout ── */
    .game-layout { display:flex;flex-direction:column;gap:1rem; }

    /* ── Stats Panel ── */
    .stats-panel {
      background:var(--surface);border:1px solid var(--border);
      border-radius:20px;padding:1.2rem;display:flex;flex-direction:column;gap:.9rem;
    }
    .avatar-row { display:flex;align-items:center;gap:.8rem; }
    .avatar {
      width:56px;height:56px;background:var(--card);border:2px solid var(--border);
      border-radius:50%;display:flex;align-items:center;justify-content:center;
      font-size:1.8rem;flex-shrink:0;
    }
    .char-info { flex:1;min-width:0; }
    .char-name { font-size:1.1rem;font-weight:700;color:var(--text);white-space:nowrap;overflow:hidden;text-overflow:ellipsis; }
    .char-class { font-size:.8rem;color:var(--muted);text-transform:capitalize; }
    .char-title { font-size:.78rem;color:var(--accent);font-style:italic; }

    .level-badge {
      background:linear-gradient(135deg,var(--primary),#e67e22);
      color:#0d0d14;font-weight:900;font-size:.9rem;
      padding:.35rem .75rem;border-radius:20px;flex-shrink:0;
    }

    /* XP Bar */
    .xp-section {}
    .xp-label {
      display:flex;justify-content:space-between;font-size:.78rem;color:var(--muted);margin-bottom:.3rem;
    }
    .xp-track {
      height:14px;background:var(--card);border-radius:10px;overflow:hidden;
      border:1px solid var(--border);
    }
    .xp-bar-fill {
      height:100%;width:0%;
      background:linear-gradient(90deg,var(--primary),#f39c12,#e67e22);
      border-radius:10px;transition:width .8s cubic-bezier(.4,0,.2,1);
    }

    /* Streak */
    .streak-section {}
    .streak-row { display:flex;align-items:center;justify-content:space-between; }
    .streak-label { font-size:.9rem;color:var(--streak);font-weight:600; }
    .multiplier-badge {
      font-size:.85rem;font-weight:700;color:var(--primary);
      background:color-mix(in srgb,var(--primary) 15%,var(--card));
      border:1px solid var(--primary);border-radius:20px;padding:.15rem .6rem;
    }

    /* Stats Row */
    .stats-row { display:grid;grid-template-columns:repeat(3,1fr);gap:.6rem; }
    .stat-item { text-align:center;background:var(--card);border-radius:12px;padding:.6rem; }
    .stat-val { font-size:1.3rem;font-weight:900;color:var(--primary); }
    .stat-key { font-size:.7rem;color:var(--muted);letter-spacing:.5px;text-transform:uppercase; }

    /* Perks */
    .perks-section {}
    .perks-title { font-size:.78rem;color:var(--accent);letter-spacing:1px;text-transform:uppercase;margin-bottom:.5rem; }
    .perks-list { display:flex;flex-wrap:wrap;gap:.5rem; }
    .perks-empty { font-size:.8rem;color:var(--muted);font-style:italic; }
    .perk-chip {
      background:color-mix(in srgb,var(--accent) 12%,var(--card));
      border:1px solid color-mix(in srgb,var(--accent) 40%,transparent);
      border-radius:20px;padding:.25rem .7rem;font-size:.75rem;
      display:flex;align-items:center;gap:.4rem;
    }
    .perk-name { font-weight:700;color:var(--accent); }
    .perk-desc { color:var(--muted); }

    /* Settings */
    .settings-row { display:flex;gap:.5rem;justify-content:flex-end; }
    .icon-btn {
      background:var(--card);border:1px solid var(--border);border-radius:8px;
      width:36px;height:36px;font-size:1rem;cursor:pointer;
      display:flex;align-items:center;justify-content:center;
      transition:all .2s;color:var(--text);
    }
    .icon-btn:hover { border-color:var(--accent);background:color-mix(in srgb,var(--accent) 15%,var(--card)); }

    /* ── Quest Panel ── */
    .quest-panel { display:flex;flex-direction:column;gap:.8rem; }
    .panel-title { font-size:1.1rem;font-weight:700;color:var(--text); }

    /* Add Task Form */
    .add-task-form {
      display:flex;flex-direction:column;gap:.6rem;
      background:var(--surface);border:1px solid var(--border);border-radius:16px;padding:1rem;
    }
    #task-input {
      width:100%;padding:.75rem 1rem;font-size:.95rem;
      background:var(--card);border:2px solid var(--border);border-radius:10px;
      color:var(--text);outline:none;transition:border-color .2s;
    }
    #task-input:focus { border-color:var(--accent); }
    #task-difficulty {
      width:100%;padding:.6rem .8rem;font-size:.9rem;
      background:var(--card);border:2px solid var(--border);border-radius:10px;
      color:var(--text);outline:none;cursor:pointer;
    }
    .add-btn {
      width:100%;padding:.75rem;font-size:1rem;font-weight:700;
      background:linear-gradient(135deg,var(--success),#16a085);
      color:#fff;border:none;border-radius:10px;cursor:pointer;
      transition:all .2s;box-shadow:0 4px 14px rgba(32,191,107,.3);
    }
    .add-btn:hover { transform:translateY(-1px);box-shadow:0 6px 20px rgba(32,191,107,.45); }

    /* Task List */
    .task-list { display:flex;flex-direction:column;gap:.5rem; }
    .task-card {
      display:flex;align-items:center;gap:.7rem;
      background:var(--surface);border:1px solid var(--border);border-radius:14px;
      padding:.75rem .9rem;transition:all .2s;
    }
    .task-card:hover { border-color:var(--accent); }
    .task-check {
      background:none;border:none;cursor:pointer;flex-shrink:0;
      font-size:1.4rem;color:var(--success);transition:transform .2s;
    }
    .task-check:hover { transform:scale(1.2); }
    .task-info { flex:1;min-width:0; }
    .task-title {
      font-size:.95rem;color:var(--text);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;
    }
    .task-meta { display:flex;gap:.4rem;margin-top:.25rem;flex-wrap:wrap; }
    .diff-badge { font-size:.7rem;padding:.1rem .5rem;border-radius:20px;font-weight:600; }
    .diff-easy { background:rgba(32,191,107,.15);color:var(--success); }
    .diff-medium { background:rgba(247,183,49,.15);color:var(--primary); }
    .diff-hard { background:rgba(230,126,34,.15);color:#e67e22; }
    .diff-epic { background:rgba(235,59,90,.15);color:var(--danger); }
    .xp-chip { font-size:.7rem;padding:.1rem .5rem;border-radius:20px;background:var(--card);color:var(--primary);font-weight:600; }
    .streak-chip { font-size:.7rem;padding:.1rem .5rem;border-radius:20px;background:rgba(253,121,168,.12);color:var(--streak); }
    .task-delete {
      background:none;border:none;cursor:pointer;font-size:.9rem;
      color:var(--muted);padding:.3rem;transition:color .2s;flex-shrink:0;
    }
    .task-delete:hover { color:var(--danger); }

    /* Empty State */
    .empty-state {
      display:flex;flex-direction:column;align-items:center;
      padding:2.5rem 1rem;color:var(--muted);text-align:center;gap:.4rem;
    }
    .empty-icon { font-size:3rem;margin-bottom:.5rem;opacity:.6; }
    .empty-text { font-size:1rem;font-weight:600; }
    .empty-sub { font-size:.85rem;opacity:.7; }

    /* Responsive */
    @media (max-width:480px) {
      .class-grid { grid-template-columns:1fr; }
      #app { padding:.8rem .6rem 3rem; }
    }

    /* Accessibility */
    button:focus-visible, input:focus-visible, select:focus-visible {
      outline:2px solid var(--accent);outline-offset:2px;
    }
  `;

  const style = document.createElement('style');
  style.id = 'questdo-styles';
  style.textContent = css;
  document.head.appendChild(style);
};

// ── Boot ──────────────────────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', initApp);
