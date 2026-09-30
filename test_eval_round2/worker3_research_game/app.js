/**
 * Todo Quest — Game Engine
 * State management, rendering, celebrations, localStorage persistence.
 */

// ============================================================
// CONSTANTS (from prd.md)
// ============================================================
const STORAGE_KEY = 'todoquest_save';
const SCHEMA_VERSION = '1.0';

const BASE_XP = 50;
const MAX_STREAK_MULTIPLIER = 3.0;
const STREAK_BONUS_PER_DAY = 0.1;

// Level thresholds: 50 * n * (n+1), giving [0, 100, 300, 600, 1000, 1500...]
function buildLevelThresholds(maxLevel = 30) {
  const thresholds = [0];
  for (let n = 1; n <= maxLevel; n++) {
    thresholds.push(Math.floor(50 * n * (n + 1)));
  }
  return thresholds;
}

const LEVEL_THRESHOLDS = buildLevelThresholds(30);

const LEVEL_TITLES = [
  'Novice Adventurer',      // 1
  'Apprentice',             // 2
  'Journeyman',             // 3
  'Skilled Seeker',         // 4
  'Seasoned Hero',          // 5
  'Veteran Explorer',       // 6
  'Elite Champion',         // 7
  'Master Tactician',       // 8
  'Grand Strategist',       // 9
  'Legendary Legend',       // 10
  'Mythic Overlord',        // 11
  'Ascended Sage',          // 12
  'Celestial Guardian',     // 13
  'Eternal Archon',        // 14
  'Divine Overmind',        // 15+
];

// Streak badge milestones
const STREAK_BADGES = [
  { days: 3,   id: 'streak_3',   label: '3-Day' },
  { days: 7,   id: 'streak_7',   label: '7-Day' },
  { days: 14,  id: 'streak_14',  label: '14-Day' },
  { days: 30,  id: 'streak_30',  label: '30-Day' },
  { days: 100, id: 'streak_100', label: '100-Day' },
];

// Level badge milestones
const LEVEL_BADGES = [5, 10, 15, 20, 25, 30].map(lvl => ({
  id: `level_${lvl}`,
  label: `Lv ${lvl}`,
}));

// Quest count badges
const QUEST_BADGES = [10, 50, 100, 500].map(n => ({
  id: `quests_${n}`,
  label: `${n} Quests`,
}));

const ALL_BADGES = [
  ...STREAK_BADGES,
  ...LEVEL_BADGES,
  ...QUEST_BADGES,
];

// Lore quotes (cycle randomly)
const LORE_QUOTES = [
  '"A journey of a thousand miles begins with a single quest."',
  '"The hero is not the one who never falls, but the one who rises after every fall."',
  '"Even the mightiest quest begins with a single step."',
  '"True legends are forged through daily discipline."',
  '"A quest taken is a battle half-won."',
  '"Persistence carves paths where talent alone cannot."',
  '"The fire of a streak burns brightest when fed daily."',
  '"He who completes his quests shall inherit the earth."',
  '"Courage is not the absence of fear, but the will to keep going."',
  '"Every master was once a beginner. Every legend, a novice."',
];

// ============================================================
// GAME STATE
// ============================================================
class GameState {
  constructor() {
    this.xp = 0;
    this.level = 1;
    this.title = LEVEL_TITLES[0];
    this.totalQuestsCompleted = 0;
    this.currentStreak = 0;
    this.longestStreak = 0;
    this.lastActiveDate = null; // ISO date string 'YYYY-MM-DD' in UTC
    this.badges = [];
    this.quests = []; // Array of { id, title, completed, createdAt }
  }

  // ---- XP / Level Math ----
  getXpForLevel(lvl) {
    if (lvl < 0 || lvl >= LEVEL_THRESHOLDS.length) return LEVEL_THRESHOLDS[LEVEL_THRESHOLDS.length - 1];
    return LEVEL_THRESHOLDS[lvl] ?? 0;
  }

  getXpForNextLevel() {
    return this.getXpForLevel(this.level + 1);
  }

  xpProgressPercent() {
    const cur = this.getXpForLevel(this.level);
    const nxt = this.getXpForLevel(this.level + 1);
    if (nxt === cur) return 100;
    return Math.min(100, Math.max(0,
      Math.round(((this.xp - cur) / (nxt - cur)) * 100)
    ));
  }

  // ---- Streak Math ----
  streakMultiplier() {
    return Math.min(1.0 + this.currentStreak * STREAK_BONUS_PER_DAY, MAX_STREAK_MULTIPLIER);
  }

  xpForQuest() {
    return Math.floor(BASE_XP * this.streakMultiplier());
  }

  // ---- Date Utilities ----
  getTodayUTC() {
    const now = new Date();
    return now.toISOString().slice(0, 10); // 'YYYY-MM-DD'
  }

  getYesterdayUTC() {
    const d = new Date();
    d.setUTCDate(d.getUTCDate() - 1);
    return d.toISOString().slice(0, 10);
  }

  // ---- Badge Management ----
  hasBadge(id) {
    return this.badges.includes(id);
  }

  awardBadge(id) {
    if (!this.hasBadge(id)) {
      this.badges.push(id);
      return true; // newly awarded
    }
    return false;
  }

  checkAndAwardBadges() {
    const newlyAwarded = [];

    // Streak badges
    for (const b of STREAK_BADGES) {
      if (this.currentStreak >= b.days) {
        if (this.awardBadge(b.id)) newlyAwarded.push(b);
      }
    }

    // Level badges
    for (const b of LEVEL_BADGES) {
      const lvlNum = parseInt(b.id.split('_')[1]);
      if (this.level >= lvlNum) {
        if (this.awardBadge(b.id)) newlyAwarded.push(b);
      }
    }

    // Quest count badges
    for (const b of QUEST_BADGES) {
      const count = parseInt(b.id.split('_')[1]);
      if (this.totalQuestsCompleted >= count) {
        if (this.awardBadge(b.id)) newlyAwarded.push(b);
      }
    }

    return newlyAwarded;
  }

  // ---- Apply XP and Level Up ----
  addXp(amount) {
    this.xp += amount;
    const prevLevel = this.level;
    while (
      this.level < LEVEL_THRESHOLDS.length - 1 &&
      this.xp >= this.getXpForLevel(this.level + 1)
    ) {
      this.level++;
    }
    this.title = LEVEL_TITLES[Math.min(this.level - 1, LEVEL_TITLES.length - 1)];

    const leveledUp = this.level > prevLevel;
    return { leveledUp, newLevel: this.level };
  }

  // ---- Update Streak ----
  updateStreakOnLoad() {
    const today = this.getTodayUTC();
    const yesterday = this.getYesterdayUTC();

    if (!this.lastActiveDate) {
      // First time ever — streak stays at 0, no penalty
      return;
    }

    if (this.lastActiveDate === today) {
      // Already active today, streak unchanged
      return;
    }

    if (this.lastActiveDate === yesterday) {
      // Streak continues (was active yesterday)
      return;
    }

    // Missed a day — streak breaks
    const hadStreak = this.currentStreak > 0;
    this.currentStreak = 0;
    if (hadStreak) {
      audio.playStreakBreak();
      this.showStreakBrokenNotice();
    }
  }

  onQuestCompleted() {
    const today = this.getTodayUTC();
    const yesterday = this.getYesterdayUTC();

    // Advance streak if this is the first completion today
    if (this.lastActiveDate !== today) {
      if (this.lastActiveDate === yesterday || this.lastActiveDate === null) {
        // Streak continues or starts fresh
        this.currentStreak++;
      } else {
        // Missed days, streak reset
        if (this.currentStreak > 0) {
          audio.playStreakBreak();
        }
        this.currentStreak = 1;
      }
      this.lastActiveDate = today;

      if (this.currentStreak > this.longestStreak) {
        this.longestStreak = this.currentStreak;
      }
    }
  }

  // ---- Add Quest ----
  addQuest(title) {
    const quest = {
      id: crypto.randomUUID(),
      title: title.trim(),
      completed: false,
      createdAt: new Date().toISOString(),
    };
    this.quests.push(quest);
    return quest;
  }

  // ---- Complete Quest ----
  completeQuest(id) {
    const idx = this.quests.findIndex(q => q.id === id);
    if (idx === -1) return null;
    const quest = this.quests[idx];
    this.quests.splice(idx, 1);
    this.totalQuestsCompleted++;
    this.onQuestCompleted();

    const xpGained = this.xpForQuest();
    const { leveledUp } = this.addXp(xpGained);
    const newBadges = this.checkAndAwardBadges();

    return { quest, xpGained, leveledUp, newBadges };
  }

  // ---- Serialization ----
  toJSON() {
    return {
      version: SCHEMA_VERSION,
      player: {
        xp: this.xp,
        level: this.level,
        title: this.title,
        total_quests_completed: this.totalQuestsCompleted,
        current_streak: this.currentStreak,
        longest_streak: this.longestStreak,
        last_active_date: this.lastActiveDate,
        badges: this.badges,
      },
      quests: this.quests,
    };
  }

  static fromJSON(data) {
    const s = new GameState();
    if (!data || !data.player) return s;

    s.xp                  = data.player.xp ?? 0;
    s.level               = data.player.level ?? 1;
    s.title               = data.player.title ?? LEVEL_TITLES[0];
    s.totalQuestsCompleted = data.player.total_quests_completed ?? 0;
    s.currentStreak       = data.player.current_streak ?? 0;
    s.longestStreak       = data.player.longest_streak ?? 0;
    s.lastActiveDate       = data.player.last_active_date ?? null;
    s.badges              = data.player.badges ?? [];
    s.quests              = (data.quests ?? []).map(q => ({
      id: q.id,
      title: q.title,
      completed: q.completed ?? false,
      createdAt: q.created_at ?? q.createdAt ?? new Date().toISOString(),
    }));
    return s;
  }

  // ---- Notifications ----
  showStreakBrokenNotice() {
    renderCelebration('streak_break', 'Streak Lost', 'A new day brings new possibilities. Start fresh!');
  }
}

// ============================================================
// GAME CONTROLLER
// ============================================================
class GameController {
  constructor() {
    this.state = null;
    this._xpAnimationFrame = null;
  }

  init() {
    this.state = this.loadState();
    this.state.updateStreakOnLoad();
    this.bindEvents();
    this.render();
    this.startLoreRotation();
  }

  // ---- Persistence ----
  loadState() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) {
        const data = JSON.parse(raw);
        return GameState.fromJSON(data);
      }
    } catch (e) {
      console.warn('[TodoQuest] Failed to load save:', e);
    }
    return new GameState();
  }

  save() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.state.toJSON()));
    } catch (e) {
      console.warn('[TodoQuest] Failed to save:', e);
    }
  }

  reset() {
    if (!confirm('Reset all progress? This cannot be undone.')) return;
    localStorage.removeItem(STORAGE_KEY);
    this.state = new GameState();
    this.save();
    this.render();
    audio.playClick();
  }

  // ---- Event Binding ----
  bindEvents() {
    // Quest form submit
    document.getElementById('questForm').addEventListener('submit', e => {
      e.preventDefault();
      this.handleAddQuest();
    });

    // Quest list delegation (complete / delete)
    document.getElementById('questList').addEventListener('change', e => {
      if (e.target.matches('.quest-checkbox')) {
        this.handleCompleteQuest(e.target);
      }
    });

    // Also handle click for completion (accessibility)
    document.getElementById('questList').addEventListener('click', e => {
      const checkbox = e.target.closest('.quest-card')?.querySelector('.quest-checkbox');
      if (checkbox && e.target !== checkbox) {
        checkbox.checked = !checkbox.checked;
        if (checkbox.checked) this.handleCompleteQuest(checkbox);
      }
    });

    // Reset button
    document.getElementById('resetBtn').addEventListener('click', () => this.reset());

    // Level-up modal close
    document.getElementById('levelupCloseBtn').addEventListener('click', () => {
      closeLevelUpModal();
    });

    // Keyboard: Enter in quest input triggers submit
    document.getElementById('questInput').addEventListener('keydown', e => {
      if (e.key === 'Enter') {
        e.preventDefault();
        this.handleAddQuest();
      }
    });

    // Input interaction sound
    document.getElementById('questInput').addEventListener('focus', () => audio.playClick());
    document.getElementById('acceptBtn').addEventListener('mouseenter', () => audio.playClick());
  }

  // ---- Quest Actions ----
  handleAddQuest() {
    const input = document.getElementById('questInput');
    const title = input.value.trim();
    if (!title) {
      input.focus();
      return;
    }

    audio.playClick();
    const quest = this.state.addQuest(title);
    this.save();
    this.render();
    input.value = '';
    input.focus();
  }

  handleCompleteQuest(checkbox) {
    const card = checkbox.closest('.quest-card');
    if (!card || card.dataset.completing) return;

    const questId = card.dataset.id;
    const result = this.state.completeQuest(questId);

    if (!result) return;

    card.dataset.completing = '1';
    card.classList.add('completing');
    audio.playChime();

    // XP flyup
    this.spawnXpFlyup(card, result.xpGained);

    // Save immediately
    this.save();

    // After animation: re-render
    setTimeout(() => {
      this.render();
      this.updateStats();

      if (result.leveledUp) {
        audio.playLevelUp();
        this.showLevelUp(result.newLevel);
        document.getElementById('levelBadge').classList.add('pulse');
        setTimeout(() => {
          document.getElementById('levelBadge').classList.remove('pulse');
        }, 700);
      }

      // Streak milestone
      const streakMilestone = STREAK_BADGES.find(
        b => b.days === this.state.currentStreak
      );
      if (streakMilestone) {
        audio.playStreakMilestone();
        this.spawnStreakParticles();
        renderCelebration(
          'streak',
          `${streakMilestone.days}-Day Streak!`,
          `You've held your streak for ${streakMilestone.days} days!`
        );
      }

      // Badge unlocks
      result.newBadges.forEach((badge, i) => {
        setTimeout(() => {
          audio.playBadgeUnlock();
          renderCelebration(
            'badge',
            'Badge Unlocked!',
            `${badge.label} — New achievement earned!`
          );
        }, i * 600);
      });
    }, 500);
  }

  // ---- Rendering ----
  render() {
    this.updateStats();
    this.renderQuests();
    this.renderBadges();
    this.updateLoreQuote();
  }

  updateStats() {
    const s = this.state;

    document.getElementById('levelNumber').textContent = `Lv ${s.level}`;
    document.getElementById('levelTitle').textContent  = s.title;
    document.getElementById('xpCurrent').textContent  = s.xp.toLocaleString();
    document.getElementById('xpNext').textContent     = s.getXpForNextLevel().toLocaleString();
    document.getElementById('xpBarFill').style.width = `${s.xpProgressPercent()}%`;

    document.getElementById('streakCount').textContent  = s.currentStreak;
    document.getElementById('totalQuests').textContent = s.totalQuestsCompleted.toLocaleString();
    document.getElementById('bestStreak').textContent  = s.longestStreak;

    // Streak flame animation
    const flame = document.getElementById('streakFlame');
    if (s.currentStreak > 0) {
      flame.classList.add('flame-active');
    } else {
      flame.classList.remove('flame-active');
    }
  }

  renderQuests() {
    const list = document.getElementById('questList');
    const empty = document.getElementById('emptyState');
    const countLabel = document.getElementById('questCountLabel');

    if (this.state.quests.length === 0) {
      list.innerHTML = '';
      empty.classList.add('visible');
      countLabel.textContent = '0 active quests';
      return;
    }

    empty.classList.remove('visible');
    countLabel.textContent = `${this.state.quests.length} active quest${this.state.quests.length !== 1 ? 's' : ''}`;

    list.innerHTML = this.state.quests.map(q => `
      <div class="quest-card" data-id="${q.id}" role="listitem">
        <input
          type="checkbox"
          class="quest-checkbox"
          id="cb-${q.id}"
          aria-label="Complete quest: ${escHtml(q.title)}"
        />
        <label class="quest-title" for="cb-${q.id}">${escHtml(q.title)}</label>
      </div>
    `).join('');
  }

  renderBadges() {
    const grid = document.getElementById('badgesGrid');
    grid.innerHTML = ALL_BADGES.map(b => {
      const earned = this.state.hasBadge(b.id);
      return `
        <div class="badge-item ${earned ? 'earned' : 'locked'}" title="${escHtml(b.label)}">
          <div class="badge-icon">
            ${earned ? badgeIconEarned(b.id) : badgeIconLocked()}
          </div>
          <span class="badge-label">${escHtml(b.label)}</span>
        </div>
      `;
    }).join('');
  }

  updateLoreQuote() {
    const idx = Math.floor(Math.random() * LORE_QUOTES.length);
    const el = document.getElementById('footerLore');
    if (el) el.textContent = LORE_QUOTES[idx];
  }

  startLoreRotation() {
    setInterval(() => this.updateLoreQuote(), 15000);
  }

  // ---- XP Flyup ----
  spawnXpFlyup(sourceCard, xpAmount) {
    const container = document.getElementById('xpFlyups');
    const rect = sourceCard.getBoundingClientRect();
    const flyup = document.createElement('div');
    flyup.className = 'xp-flyup';
    flyup.textContent = `+${xpAmount} XP`;
    flyup.style.left = `${rect.left + rect.width / 2 - 40}px`;
    flyup.style.top  = `${rect.top}px`;
    container.appendChild(flyup);
    setTimeout(() => flyup.remove(), 1300);
  }

  // ---- Streak Particles ----
  spawnStreakParticles() {
    const canvas = document.getElementById('particleCanvas');
    const ctx = canvas.getContext('2d');
    canvas.width  = window.innerWidth;
    canvas.height = window.innerHeight;

    const colors = ['#FF6B35', '#FFD700', '#FF9F1C', '#FFCE45'];
    const particles = [];

    for (let i = 0; i < 60; i++) {
      particles.push({
        x: Math.random() * canvas.width,
        y: canvas.height + 10,
        vx: (Math.random() - 0.5) * 3,
        vy: -(Math.random() * 5 + 4),
        size: Math.random() * 6 + 2,
        color: colors[Math.floor(Math.random() * colors.length)],
        alpha: 1,
        decay: Math.random() * 0.02 + 0.01,
      });
    }

    let animFrame;
    function draw() {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      let alive = false;
      for (const p of particles) {
        p.x += p.vx;
        p.y += p.vy;
        p.vy += 0.08; // slight gravity
        p.alpha -= p.decay;
        if (p.alpha <= 0) continue;
        alive = true;
        ctx.save();
        ctx.globalAlpha = Math.max(0, p.alpha);
        ctx.fillStyle = p.color;
        ctx.beginPath();
        ctx.arc(p.x, p.y, p.size, 0, Math.PI * 2);
        ctx.fill();
        ctx.restore();
      }
      if (alive) {
        animFrame = requestAnimationFrame(draw);
      } else {
        ctx.clearRect(0, 0, canvas.width, canvas.height);
      }
    }
    draw();
    setTimeout(() => cancelAnimationFrame(animFrame), 3000);
  }

  // ---- Level Up Modal ----
  showLevelUp(newLevel) {
    // Golden flash
    const flash = document.createElement('div');
    flash.className = 'golden-flash';
    document.body.appendChild(flash);
    setTimeout(() => flash.remove(), 900);

    // Modal
    document.getElementById('levelupLevelDisplay').textContent = `Level ${newLevel}`;
    document.getElementById('levelupNewTitle').textContent    = this.state.title;
    document.getElementById('levelupXpInfo').textContent       = `Total XP: ${this.state.xp.toLocaleString()}`;
    document.getElementById('levelupModal').classList.add('active');

    // Trap focus
    document.getElementById('levelupCloseBtn').focus();
  }
}

// ============================================================
// UI HELPERS
// ============================================================
function closeLevelUpModal() {
  document.getElementById('levelupModal').classList.remove('active');
  audio.playClick();
}

function renderCelebration(type, title, subtitle) {
  const overlay = document.getElementById('celebrationOverlay');
  document.getElementById('celebrationTitle').textContent    = title;
  document.getElementById('celebrationSubtitle').textContent = subtitle;

  const icons = {
    streak:       '🔥',
    streak_break: '💔',
    badge:        '🏆',
  };
  document.getElementById('celebrationIcon').textContent = icons[type] || '✨';

  overlay.classList.add('active');
  setTimeout(() => overlay.classList.remove('active'), 2200);
}

function escHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

function badgeIconEarned(id) {
  const type = id.split('_')[0];
  if (type === 'streak') {
    return `<svg viewBox="0 0 24 24" fill="none" width="24" height="24">
      <path d="M12 2C12 2 7 7 7 12C7 14.76 8.79 17.14 11 18.18C10.27 16.82 10 15.22 10.5 13.5C10.8 14.2 11.5 14.7 12.5 14.7C13.5 14.7 14.2 14.2 14.5 13.5C14.8 14.2 15.5 14.7 16.5 14.7C17.5 14.7 18.2 14.2 18.5 13.5C19 15.22 18.73 16.82 18 18.18C20.21 17.14 22 14.76 22 12C22 7 17 2 17 2" stroke="#FF6B35" stroke-width="1.5" fill="rgba(255,107,53,0.15)"/>
    </svg>`;
  }
  if (type === 'level') {
    return `<svg viewBox="0 0 24 24" fill="none" width="24" height="24">
      <polygon points="12,2 15.09,8.26 22,9.27 17,14.14 18.18,21.02 12,17.77 5.82,21.02 7,14.14 2,9.27 8.91,8.26" stroke="#D4A017" stroke-width="1.5" fill="rgba(212,160,23,0.2)"/>
    </svg>`;
  }
  if (type === 'quests') {
    return `<svg viewBox="0 0 24 24" fill="none" width="24" height="24">
      <path d="M9 12l2 2 4-4" stroke="#4ADE80" stroke-width="2" stroke-linecap="round"/>
      <rect x="3" y="4" width="18" height="18" rx="3" stroke="#4ADE80" stroke-width="1.5"/>
    </svg>`;
  }
  return `<svg viewBox="0 0 24 24" fill="none" width="24" height="24">
    <circle cx="12" cy="12" r="9" stroke="#D4A017" stroke-width="1.5"/>
  </svg>`;
}

function badgeIconLocked() {
  return `<svg viewBox="0 0 24 24" fill="none" width="24" height="24">
    <rect x="6" y="10" width="12" height="10" rx="2" stroke="#5C544A" stroke-width="1.5"/>
    <path d="M9 10V7a3 3 0 0 1 6 0v3" stroke="#5C544A" stroke-width="1.5" stroke-linecap="round"/>
  </svg>`;
}

// ============================================================
// BOOT
// ============================================================
let game;
document.addEventListener('DOMContentLoaded', () => {
  game = new GameController();
  game.init();
});
