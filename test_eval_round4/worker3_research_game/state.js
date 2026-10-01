/**
 * QuestDo — Game State Manager
 * Handles player, tasks, XP, levels, streaks, perks, localStorage
 */

const GameState = (() => {
  const STORAGE_KEY_PLAYER = 'questdo_player';
  const STORAGE_KEY_TASKS  = 'questdo_tasks';
  const STORAGE_KEY_STREAK = 'questdo_streak';

  // ── Difficulty XP Map ────────────────────────────────────────────────────
  const XP_MAP = { easy: 10, medium: 20, hard: 30, epic: 50 };

  // ── XP required per level (exponential curve) ────────────────────────────
  const xpForLevel = (n) => Math.floor(50 * Math.pow(1.5, n - 1));

  // ── Class Definitions ────────────────────────────────────────────────────
  const CLASS_DATA = {
    warrior: {
      name: 'Warrior',
      icon: '⚔️',
      desc: 'Masters of endurance. Earn +20% XP on Hard & Epic tasks.',
      color: '#eb3b5a',
      bonus: (xp, diff) => (diff === 'hard' || diff === 'epic') ? xp * 1.2 : xp,
      perkBonus: { epic: 5 }
    },
    mage: {
      name: 'Mage',
      icon: '🧙',
      desc: 'Seekers of wisdom. Reach perks at 80% of normal XP thresholds.',
      color: '#a29bfe',
      bonus: (xp) => xp,
      perkThreshold: 0.8
    },
    rogue: {
      name: 'Rogue',
      icon: '🗡️',
      desc: 'Swift and cunning. Streak multipliers activate 30% sooner.',
      color: '#20bf6b',
      bonus: (xp) => xp,
      streakActivation: 0.7
    }
  };

  // ── Perks by Class ───────────────────────────────────────────────────────
  const PERKS = {
    warrior: [
      { level: 2, name: '⚡ Power Strike',   desc: '+5 bonus XP on Epic tasks',    active: false },
      { level: 5, name: '🛡️ Iron Will',      desc: '1 free streak-save per week',   active: false },
      { level: 8, name: '🔥 Battle Fury',   desc: '2× XP on Hard tasks (Tue)',     active: false }
    ],
    mage: [
      { level: 2, name: '📖 Quick Study',    desc: 'Start at Lv2 with +25 bonus XP', active: false },
      { level: 5, name: '✨ Arcane Wisdom',  desc: '1.5× XP on Medium tasks',        active: false },
      { level: 8, name: '🔮 Spell Master',  desc: 'Meditation type: 2× streak mult', active: false }
    ],
    rogue: [
      { level: 2, name: '👟 Swift Feet',     desc: 'Streak mult. activates at 5 days', active: false },
      { level: 5, name: '🍀 Lucky Strike',   desc: '10% chance of 3× random bonus XP', active: false },
      { level: 8, name: '💨 Shadow Step',    desc: '3 easy tasks = 1 free Epic',       active: false }
    ]
  };

  const TITLE_BRACKETS = [
    { min: 1,  max: 4,  title: 'Apprentice' },
    { min: 5,  max: 8,  title: 'Adept' },
    { min: 9,  max: 12, title: 'Master' },
    { min: 13, max: 999, title: 'Legend' }
  ];

  // ── Default State ─────────────────────────────────────────────────────────
  const defaultPlayer = () => ({
    name: 'Hero',
    class: 'warrior',
    xp: 0,
    level: 1,
    totalTasksCompleted: 0,
    longestStreak: 0,
    badges: [],
    perks: [],
    weeklyStreakSaves: 0,
    lastPerkLevel: 0
  });

  const defaultStreak = () => ({
    dates: [],
    multiplier: 1.0,
    isNewDay: false
  });

  // ── Internal State ───────────────────────────────────────────────────────
  let player = defaultPlayer();
  let tasks = [];
  let streak = defaultStreak();
  let listeners = [];

  // ── Utility ──────────────────────────────────────────────────────────────
  const today = () => new Date().toISOString().slice(0, 10);

  const generateId = () => Date.now().toString(36) + Math.random().toString(36).slice(2);

  const getTitle = () => {
    const b = TITLE_BRACKETS.find(b => player.level >= b.min && player.level <= b.max);
    return b ? b.title : 'Legend';
  };

  // ── Streak Logic ─────────────────────────────────────────────────────────
  const getStreakMultiplier = (taskStreak = 0) => {
    const cls = player.class;
    let threshold = 7;
    if (cls === 'rogue' && CLASS_DATA.rogue.streakActivation) threshold = Math.round(7 * 0.7); // 5 days

    if (taskStreak >= 30) return 3.0;
    if (taskStreak >= 14) return 2.0;
    if (taskStreak >= threshold) return 1.5;
    return 1.0;
  };

  const updateStreak = () => {
    const todayStr = today();
    if (!streak.dates.includes(todayStr)) {
      const yesterday = new Date();
      yesterday.setDate(yesterday.getDate() - 1);
      const yStr = yesterday.toISOString().slice(0, 10);

      if (streak.dates[streak.dates.length - 1] === yStr) {
        streak.dates.push(todayStr);
        streak.multiplier = getStreakMultiplier(streak.dates.length - 1);
      } else {
        // Streak broken — reset
        streak.dates = [todayStr];
        streak.multiplier = 1.0;
        if (window.AudioEngine && window.AudioEngine.playDailyReset) window.AudioEngine.playDailyReset();
      }
    }
    saveStreak();
  };

  const getStreakCount = () => streak.dates.length;

  // ── XP & Level Logic ─────────────────────────────────────────────────────
  const xpToNextLevel = (lv) => xpForLevel(lv + 1);

  const totalXPForLevel = (lv) => {
    let total = 0;
    for (let i = 2; i <= lv; i++) total += xpForLevel(i);
    return total;
  };

  const currentLevelXP = () => {
    if (player.level === 1) return player.xp;
    return player.xp - totalXPForLevel(player.level - 1);
  };

  const xpPercent = () => {
    const needed = xpForLevel(player.level + 1);
    return Math.min(100, (currentLevelXP() / needed) * 100);
  };

  const checkLuckyStrike = () => {
    if (player.class !== 'rogue') return false;
    const perk = PERKS.rogue.find(p => p.level === 2);
    return perk && perk.active && Math.random() < 0.10;
  };

  const gainXP = (amount) => {
    const oldLevel = player.level;
    player.xp += amount;

    while (player.xp >= totalXPForLevel(player.level + 1)) {
      player.level++;
    }

    savePlayer();
    return player.level > oldLevel ? player.level - oldLevel : 0;
  };

  const checkPerkUnlock = () => {
    const classPerks = PERKS[player.class] || [];
    const newlyUnlocked = [];

    classPerks.forEach(perk => {
      if (player.level >= perk.level && !perk.active) {
        perk.active = true;
        if (!player.perks.find(p => p.name === perk.name)) {
          player.perks.push({ ...perk });
          newlyUnlocked.push(perk);
        }
      }
    });

    if (newlyUnlocked.length > 0) {
      player.lastPerkLevel = player.level;
      savePlayer();
    }

    return newlyUnlocked[0] || null;
  };

  // ── Task Operations ──────────────────────────────────────────────────────
  const addTask = (title, difficulty) => {
    const xp = XP_MAP[difficulty] || 10;
    const task = {
      id: generateId(),
      title,
      difficulty,
      xp,
      completed: false,
      streak: 0,
      createdAt: today(),
      completedAt: null
    };
    tasks.push(task);
    saveTasks();
    return task;
  };

  const completeTask = (id) => {
    const idx = tasks.findIndex(t => t.id === id);
    if (idx === -1 || tasks[idx].completed) return { earned: 0, leveled: 0 };

    const task = tasks[idx];
    task.completed = true;
    task.completedAt = today();
    task.streak++;

    let earned = task.xp;

    // Apply class bonus
    const clsData = CLASS_DATA[player.class];
    earned = Math.round(clsData.bonus(earned, task.difficulty));

    // Warrior +5 on Epic at Lv2
    if (player.class === 'warrior' && task.difficulty === 'epic') {
      const perk = PERKS.warrior.find(p => p.level === 2);
      if (perk && perk.active) earned += 5;
    }

    // Mage 1.5× on Medium at Lv5
    if (player.class === 'mage' && task.difficulty === 'medium') {
      const perk = PERKS.mage.find(p => p.level === 5);
      if (perk && perk.active) earned = Math.round(earned * 1.5);
    }

    // Tuesday double XP for Warrior
    if (player.class === 'warrior' && new Date().getDay() === 2) {
      const perk = PERKS.warrior.find(p => p.level === 8);
      if (perk && perk.active && task.difficulty === 'hard') earned *= 2;
    }

    // Lucky Strike (10% 3×)
    if (checkLuckyStrike()) {
      earned *= 3;
      AudioEngine.playStreakBonus();
    }

    // Apply streak multiplier (task-level)
    const mult = getStreakMultiplier(task.streak);
    earned = Math.round(earned * mult);

    // Check global streak
    if (!streak.dates.includes(today())) {
      updateStreak();
    }
    const globalMult = getStreakMultiplier(streak.dates.length);
    earned = Math.round(earned * globalMult);

    // Gain XP
    const leveled = gainXP(earned);

    // Check perks
    const newPerk = checkPerkUnlock();

    // Update stats
    player.totalTasksCompleted++;
    if (streak.dates.length > player.longestStreak) {
      player.longestStreak = streak.dates.length;
    }

    savePlayer();
    saveTasks();

    return { earned, leveled, mult, newPerk };
  };

  const deleteTask = (id) => {
    tasks = tasks.filter(t => t.id !== id);
    saveTasks();
  };

  const getTasks = () => tasks;

  // ── Persistence ──────────────────────────────────────────────────────────
  const savePlayer = () => {
    try { localStorage.setItem(STORAGE_KEY_PLAYER, JSON.stringify(player)); } catch(e) {}
  };

  const saveTasks = () => {
    try { localStorage.setItem(STORAGE_KEY_TASKS, JSON.stringify(tasks)); } catch(e) {}
  };

  const saveStreak = () => {
    try { localStorage.setItem(STORAGE_KEY_STREAK, JSON.stringify(streak)); } catch(e) {}
  };

  const loadAll = () => {
    try {
      const p = localStorage.getItem(STORAGE_KEY_PLAYER);
      if (p) player = { ...defaultPlayer(), ...JSON.parse(p) };

      const t = localStorage.getItem(STORAGE_KEY_TASKS);
      if (t) tasks = JSON.parse(t);

      const s = localStorage.getItem(STORAGE_KEY_STREAK);
      if (s) streak = { ...defaultStreak(), ...JSON.parse(s) };
    } catch(e) {}
  };

  const initPlayer = (name, cls) => {
    player = defaultPlayer();
    player.name = name || 'Hero';
    player.class = cls || 'warrior';

    // Mage starts with bonus XP
    if (player.class === 'mage') {
      const perk = PERKS.mage.find(p => p.level === 2);
      if (perk) perk.active = true;
      player.xp = 25;
    }

    streak = defaultStreak();
    streak.dates = [today()];
    streak.multiplier = 1.0;

    savePlayer();
    saveStreak();
    saveTasks();
  };

  const resetGame = () => {
    player = defaultPlayer();
    tasks = [];
    streak = defaultStreak();
    localStorage.removeItem(STORAGE_KEY_PLAYER);
    localStorage.removeItem(STORAGE_KEY_TASKS);
    localStorage.removeItem(STORAGE_KEY_STREAK);
  };

  const exportState = () => JSON.stringify({ player, tasks, streak }, null, 2);

  const importState = (json) => {
    try {
      const d = JSON.parse(json);
      if (d.player) { player = d.player; savePlayer(); }
      if (d.tasks)  { tasks = d.tasks;  saveTasks();  }
      if (d.streak) { streak = d.streak; saveStreak(); }
      notify();
    } catch(e) { if (window.AudioEngine && window.AudioEngine.playError) window.AudioEngine.playError(); }
  };

  const notify = () => listeners.forEach(fn => fn());

  const onUpdate = (fn) => { listeners.push(fn); };

  // ── Stats Getters ────────────────────────────────────────────────────────
  const getPlayer = () => player;
  const getStreak = () => streak;
  const getClassData = () => CLASS_DATA;
  const getPerks = () => PERKS[player.class] || [];

  return {
    initPlayer, resetGame, addTask, completeTask, deleteTask, getTasks,
    gainXP, xpForLevel, xpToNextLevel, xpPercent, currentLevelXP,
    getStreakCount, getStreakMultiplier, getStreak,
    getPlayer, getClassData, getPerks, getTitle,
    loadAll, savePlayer, saveTasks, saveStreak,
    exportState, importState, onUpdate, notify
  };
})();
