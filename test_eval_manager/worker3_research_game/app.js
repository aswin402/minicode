/**
 * Todo Quest - Main Application
 * Gamified Task Productivity Game
 */

import { 
  initAudio, 
  playCompletionSound, 
  playLevelUpSound, 
  playStreakMilestoneSound,
  playClickSound 
} from './audio.js';

// ============================================
// Game Configuration
// ============================================

const CONFIG = {
  STORAGE_KEY: 'todoQuest',
  STREAK_HOURS: 24,
  STREAK_MILESTONES: [7, 30, 100],
  TIERS: {
    1: { name: 'Minor', xp: 10, color: '#9CA3AF' },
    2: { name: 'Simple', xp: 20, color: '#22C55E' },
    3: { name: 'Heroic', xp: 35, color: '#3B82F6' },
    4: { name: 'Epic', xp: 50, color: '#A855F7' }
  }
};

// ============================================
// Utility Functions
// ============================================

function generateId() {
  return Date.now().toString(36) + Math.random().toString(36).substr(2);
}

function getToday() {
  return new Date().toISOString().split('T')[0];
}

function calculateLevel(totalXP) {
  return Math.floor(Math.sqrt(totalXP / 100)) + 1;
}

function xpForLevel(level) {
  return Math.pow(level - 1, 2) * 100;
}

function xpForNextLevel(totalXP) {
  const currentLevel = calculateLevel(totalXP);
  const currentLevelXP = xpForLevel(currentLevel);
  const nextLevelXP = xpForLevel(currentLevel + 1);
  return { current: totalXP - currentLevelXP, needed: nextLevelXP - currentLevelXP };
}

function getStreakMultiplier(streakDays) {
  return Math.min(1 + (streakDays * 0.1), 2);
}

function isStreakBroken(lastCompletion) {
  if (!lastCompletion) return true;
  const last = new Date(lastCompletion);
  const now = new Date();
  const hoursSince = (now - last) / (1000 * 60 * 60);
  return hoursSince > CONFIG.STREAK_HOURS;
}

// ============================================
// State Management
// ============================================

const DEFAULT_STATE = {
  character: {
    name: 'Adventurer',
    totalXP: 0
  },
  streak: {
    current: 0,
    lastCompletion: null
  },
  tasks: [],
  stats: {
    totalCompleted: 0,
    tasksToday: 0,
    longestStreak: 0
  },
  dailyResetAt: getToday()
};

let gameState = { ...DEFAULT_STATE };

function loadState() {
  try {
    const saved = localStorage.getItem(CONFIG.STORAGE_KEY);
    if (saved) {
      const parsed = JSON.parse(saved);
      // Merge with defaults to handle schema updates
      gameState = {
        ...DEFAULT_STATE,
        ...parsed,
        character: { ...DEFAULT_STATE.character, ...parsed.character },
        streak: { ...DEFAULT_STATE.streak, ...parsed.streak },
        stats: { ...DEFAULT_STATE.stats, ...parsed.stats }
      };
    }
  } catch (e) {
    console.warn('Failed to load state:', e);
    gameState = { ...DEFAULT_STATE };
  }
  
  // Check for daily reset
  checkDailyReset();
  
  // Check for broken streak
  if (isStreakBroken(gameState.streak.lastCompletion) && gameState.streak.current > 0) {
    gameState.streak.current = 0;
    saveState();
  }
}

function saveState() {
  try {
    localStorage.setItem(CONFIG.STORAGE_KEY, JSON.stringify(gameState));
  } catch (e) {
    console.warn('Failed to save state:', e);
  }
}

function checkDailyReset() {
  const today = getToday();
  if (gameState.dailyResetAt !== today) {
    gameState.stats.tasksToday = 0;
    gameState.dailyResetAt = today;
    saveState();
  }
}

// ============================================
// Confetti System
// ============================================

class ConfettiSystem {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.particles = [];
    this.running = false;
    this.resize();
    window.addEventListener('resize', () => this.resize());
  }
  
  resize() {
    this.canvas.width = window.innerWidth;
    this.canvas.height = window.innerHeight;
  }
  
  createParticle(x, y, options = {}) {
    const colors = ['#FFD700', '#FF007F', '#3B82F6', '#22C55E', '#A855F7'];
    return {
      x,
      y,
      vx: (Math.random() - 0.5) * 12,
      vy: Math.random() * -15 - 5,
      color: colors[Math.floor(Math.random() * colors.length)],
      size: Math.random() * 8 + 4,
      rotation: Math.random() * 360,
      rotationSpeed: (Math.random() - 0.5) * 10,
      gravity: 0.5,
      drag: 0.98,
      opacity: 1,
      life: 1
    };
  }
  
  burst(x, y, count = 50) {
    for (let i = 0; i < count; i++) {
      this.particles.push(this.createParticle(x, y));
    }
    if (!this.running) this.animate();
  }
  
  levelUpBurst() {
    const centerX = this.canvas.width / 2;
    const centerY = this.canvas.height / 2;
    for (let i = 0; i < 150; i++) {
      const angle = (Math.PI * 2 * i) / 150;
      const distance = Math.random() * 100;
      const x = centerX + Math.cos(angle) * distance;
      const y = centerY + Math.sin(angle) * distance;
      const particle = this.createParticle(x, y);
      particle.vx = Math.cos(angle) * (Math.random() * 10 + 5);
      particle.vy = Math.sin(angle) * (Math.random() * 10 + 5);
      this.particles.push(particle);
    }
    if (!this.running) this.animate();
  }
  
  animate() {
    this.running = true;
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    
    this.particles = this.particles.filter(p => p.life > 0.01);
    
    for (const p of this.particles) {
      p.vy += p.gravity;
      p.vx *= p.drag;
      p.vy *= p.drag;
      p.x += p.vx;
      p.y += p.vy;
      p.rotation += p.rotationSpeed;
      p.life -= 0.015;
      p.opacity = p.life;
      
      this.ctx.save();
      this.ctx.translate(p.x, p.y);
      this.ctx.rotate((p.rotation * Math.PI) / 180);
      this.ctx.globalAlpha = p.opacity;
      this.ctx.fillStyle = p.color;
      this.ctx.fillRect(-p.size / 2, -p.size / 2, p.size, p.size);
      this.ctx.restore();
    }
    
    if (this.particles.length > 0) {
      requestAnimationFrame(() => this.animate());
    } else {
      this.running = false;
    }
  }
}

const confetti = new ConfettiSystem(document.getElementById('confetti-canvas'));

// ============================================
// UI Rendering
// ============================================

function render() {
  renderHeader();
  renderStreakBar();
  renderTaskInput();
  renderTaskList();
  renderStatsFooter();
}

function renderHeader() {
  const { level, xpNeeded } = getLevelInfo();
  const { current, needed } = xpForNextLevel(gameState.character.totalXP);
  const progressPercent = (current / needed) * 100;
  
  document.getElementById('level-number').textContent = level;
  document.getElementById('xp-current').textContent = current;
  document.getElementById('xp-needed').textContent = needed;
  document.getElementById('xp-progress').style.width = `${progressPercent}%`;
}

function getLevelInfo() {
  const level = calculateLevel(gameState.character.totalXP);
  return { level };
}

function renderStreakBar() {
  const streakEl = document.getElementById('streak-count');
  const iconEl = document.getElementById('streak-icon');
  const multEl = document.getElementById('streak-multiplier');
  
  streakEl.textContent = gameState.streak.current;
  const mult = getStreakMultiplier(gameState.streak.current);
  multEl.textContent = `×${mult.toFixed(1)}`;
  
  // Add intensity class for high streaks
  if (gameState.streak.current >= 7) {
    iconEl.classList.add('intense');
  } else {
    iconEl.classList.remove('intense');
  }
}

function renderTaskInput() {
  // Tier buttons already have event listeners, just update active state
  document.querySelectorAll('.tier-btn').forEach(btn => {
    const isActive = btn.dataset.tier === '1';
    btn.classList.toggle('active', isActive);
  });
}

function renderTaskList() {
  const container = document.getElementById('task-list');
  const countEl = document.getElementById('task-count');
  
  const incompleteTasks = gameState.tasks.filter(t => !t.completed);
  const completedTasks = gameState.tasks.filter(t => t.completed);
  
  countEl.textContent = `${incompleteTasks.length} active`;
  
  if (gameState.tasks.length === 0) {
    container.innerHTML = `
      <div class="empty-state">
        <div class="empty-icon">⚔️</div>
        <h3 class="empty-title">No quests yet</h3>
        <p class="empty-text">Add your first quest to begin your adventure!</p>
      </div>
    `;
    return;
  }
  
  // Render incomplete tasks first
  const allTasks = [...incompleteTasks, ...completedTasks];
  
  container.innerHTML = allTasks.map(task => {
    const tier = CONFIG.TIERS[task.tier];
    return `
      <div class="task-card ${task.completed ? 'completed' : ''}" data-id="${task.id}" data-tier="${task.tier}">
        <button class="task-checkbox ${task.completed ? 'checked' : ''}" data-action="toggle" data-id="${task.id}"></button>
        <div class="task-info">
          <div class="task-text">${escapeHtml(task.text)}</div>
          <div class="task-meta">
            <span class="task-xp">+${tier.xp} XP</span>
            <span class="task-tier-badge" data-tier="${task.tier}">${tier.name}</span>
          </div>
        </div>
        <button class="task-delete" data-action="delete" data-id="${task.id}">✕</button>
      </div>
    `;
  }).join('');
  
  // Attach event listeners
  container.querySelectorAll('.task-checkbox').forEach(btn => {
    btn.addEventListener('click', handleToggleTask);
  });
  container.querySelectorAll('.task-delete').forEach(btn => {
    btn.addEventListener('click', handleDeleteTask);
  });
}

function renderStatsFooter() {
  document.getElementById('stat-total').textContent = gameState.stats.totalCompleted;
  document.getElementById('stat-today').textContent = gameState.stats.tasksToday;
  document.getElementById('stat-streak').textContent = gameState.stats.longestStreak;
}

function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

// ============================================
// Event Handlers
// ============================================

function handleAddTask(e) {
  e.preventDefault();
  initAudio();
  playClickSound();
  
  const input = document.getElementById('task-input');
  const text = input.value.trim();
  
  if (!text) return;
  
  const activeTier = document.querySelector('.tier-btn.active');
  const tier = activeTier ? parseInt(activeTier.dataset.tier) : 1;
  
  const task = {
    id: generateId(),
    text,
    tier,
    completed: false,
    createdAt: new Date().toISOString()
  };
  
  gameState.tasks.unshift(task);
  saveState();
  
  input.value = '';
  renderTaskList();
}

function handleToggleTask(e) {
  initAudio();
  const id = e.currentTarget.dataset.id;
  const task = gameState.tasks.find(t => t.id === id);
  
  if (!task || task.completed) return;
  
  // Mark as completed
  task.completed = true;
  
  // Calculate XP with streak multiplier
  const baseXP = CONFIG.TIERS[task.tier].xp;
  const multiplier = getStreakMultiplier(gameState.streak.current);
  const earnedXP = Math.floor(baseXP * multiplier);
  
  // Store old level for comparison
  const oldLevel = calculateLevel(gameState.character.totalXP);
  
  // Award XP
  gameState.character.totalXP += earnedXP;
  
  // Update stats
  gameState.stats.totalCompleted++;
  gameState.stats.tasksToday++;
  
  // Update streak
  gameState.streak.lastCompletion = new Date().toISOString();
  gameState.streak.current++;
  
  // Update longest streak
  if (gameState.streak.current > gameState.stats.longestStreak) {
    gameState.stats.longestStreak = gameState.streak.current;
  }
  
  // Check for streak milestones
  const isStreakMilestone = CONFIG.STREAK_MILESTONES.includes(gameState.streak.current);
  
  saveState();
  
  // Visual feedback
  const rect = e.currentTarget.getBoundingClientRect();
  showXPPopup(rect.left + rect.width / 2, rect.top, earnedXP);
  
  // Play sounds and effects
  playCompletionSound();
  confetti.burst(rect.left + rect.width / 2, rect.top, 30);
  
  // Check for level up
  const newLevel = calculateLevel(gameState.character.totalXP);
  if (newLevel > oldLevel) {
    setTimeout(() => {
      playLevelUpSound();
      showLevelUpOverlay(newLevel);
      confetti.levelUpBurst();
    }, 300);
  } else if (isStreakMilestone) {
    setTimeout(() => {
      playStreakMilestoneSound();
      showStreakMilestone(gameState.streak.current);
    }, 500);
  }
  
  // Re-render
  render();
}

function handleDeleteTask(e) {
  initAudio();
  playClickSound();
  
  const id = e.currentTarget.dataset.id;
  gameState.tasks = gameState.tasks.filter(t => t.id !== id);
  saveState();
  renderTaskList();
}

function handleTierSelect(e) {
  initAudio();
  playClickSound();
  
  document.querySelectorAll('.tier-btn').forEach(btn => btn.classList.remove('active'));
  e.currentTarget.classList.add('active');
}

function showXPPopup(x, y, amount) {
  const popup = document.createElement('div');
  popup.className = 'xp-popup';
  popup.textContent = `+${amount} XP`;
  popup.style.left = `${x}px`;
  popup.style.top = `${y}px`;
  document.body.appendChild(popup);
  
  setTimeout(() => popup.remove(), 1000);
}

function showLevelUpOverlay(level) {
  const overlay = document.createElement('div');
  overlay.className = 'level-up-overlay';
  overlay.innerHTML = `
    <div class="level-up-content">
      <h1 class="level-up-title">LEVEL UP!</h1>
      <p class="level-up-subtitle">You are now <span class="level-up-number">Level ${level}</span></p>
    </div>
    <p class="dismiss-hint">Click anywhere to continue</p>
  `;
  
  overlay.addEventListener('click', () => {
    overlay.remove();
  });
  
  document.body.appendChild(overlay);
}

function showStreakMilestone(days) {
  const overlay = document.createElement('div');
  overlay.className = 'level-up-overlay';
  overlay.innerHTML = `
    <div class="level-up-content">
      <h1 class="level-up-title">🔥 ${days} DAY STREAK!</h1>
      <p class="level-up-subtitle">Your dedication is legendary!</p>
    </div>
    <p class="dismiss-hint">Click anywhere to continue</p>
  `;
  
  overlay.addEventListener('click', () => {
    overlay.remove();
  });
  
  document.body.appendChild(overlay);
}

// ============================================
// Initialization
// ============================================

function init() {
  // Load saved state
  loadState();
  
  // Initial render
  render();
  
  // Attach event listeners
  document.getElementById('add-task-form').addEventListener('submit', handleAddTask);
  
  document.querySelectorAll('.tier-btn').forEach(btn => {
    btn.addEventListener('click', handleTierSelect);
  });
  
  // Initialize audio on first interaction
  document.addEventListener('click', initAudio, { once: true });
}

// Start the app
document.addEventListener('DOMContentLoaded', init);
