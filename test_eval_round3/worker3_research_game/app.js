/**
 * QuestDo - Gamified Todo RPG Game
 * Main application logic with XP, levels, streaks, and perks
 */

// ============================================
// GAME CONSTANTS
// ============================================

const CHARACTER_CLASSES = {
    warrior: {
        name: 'Warrior',
        icon: '🗡️',
        stats: { str: 15, int: 8, agi: 10 },
        passive: '+20% XP on hard/epic tasks'
    },
    mage: {
        name: 'Mage',
        icon: '🔮',
        stats: { str: 8, int: 15, agi: 10 },
        passive: '+INT bonus to all XP'
    },
    rogue: {
        name: 'Rogue',
        icon: '🗝️',
        stats: { str: 10, int: 10, agi: 15 },
        passive: '+15% streak multiplier'
    }
};

const BASE_XP = {
    easy: 10,
    medium: 25,
    hard: 50,
    epic: 100
};

// Level thresholds: XP required to reach that level
const LEVEL_THRESHOLDS = [0, 100, 250, 500, 900, 1450, 2275, 3513, 5370, 8155, 12333];

const PERKS = {
    quick_learner: { name: 'Quick Learner', level: 2, icon: '📚', effect: '+5% XP gain' },
    double_task: { name: 'Double Task', level: 3, icon: '✨', effect: 'First 2 tasks/day give bonus XP' },
    habit_architect: { name: 'Habit Architect', level: 5, icon: '🏗️', effect: '+10% streak multiplier' },
    power_hour: { name: 'Power Hour', level: 7, icon: '⚡', effect: 'First task each day = 2x XP' },
    master_achiever: { name: 'Master Achiever', level: 10, icon: '👑', effect: 'Unlock epic-tier tasks' }
};

// ============================================
// GAME STATE
// ============================================

class QuestDoGame {
    constructor() {
        this.state = null;
        this.autoSaveInterval = null;
    }

    /**
     * Initialize the game
     */
    init() {
        // Initialize systems
        audio.init();
        effects.init();
        
        // Load or create state
        this.state = storage.load();
        
        // Apply settings
        if (!this.state.settings.soundEnabled) {
            audio.enabled = false;
            document.getElementById('sound-toggle').textContent = '🔇';
        }
        
        // Check for new day (streak reset logic)
        this.checkNewDay();
        
        // Set up event listeners
        this.setupEventListeners();
        
        // Start auto-save
        this.startAutoSave();
        
        // Render UI
        this.render();
    }

    /**
     * Set up all event listeners
     */
    setupEventListeners() {
        // Sound toggle
        document.getElementById('sound-toggle').addEventListener('click', () => this.toggleSound());
        
        // Character selection
        document.querySelectorAll('.class-card').forEach(card => {
            card.addEventListener('click', () => this.selectClass(card.dataset.class));
        });
        
        // Confirm hero
        document.getElementById('confirm-hero').addEventListener('click', () => this.confirmCharacter());
        
        // Task form
        document.getElementById('task-form').addEventListener('submit', (e) => {
            e.preventDefault();
            this.addTask();
        });
        
        // Close celebration overlay
        document.getElementById('celebration-overlay').addEventListener('click', () => {
            document.getElementById('celebration-overlay').classList.add('hidden');
        });
        
        // Unlock epic option when reaching level 10
        if (this.state && this.state.xp.level >= 10) {
            document.getElementById('epic-option').disabled = false;
        }
    }

    /**
     * Select a character class
     */
    selectClass(classType) {
        document.querySelectorAll('.class-card').forEach(card => {
            card.classList.remove('selected');
        });
        
        const selectedCard = document.querySelector(`[data-class="${classType}"]`);
        if (selectedCard) {
            selectedCard.classList.add('selected');
            audio.play('buttonClick');
        }
    }

    /**
     * Confirm character selection
     */
    confirmCharacter() {
        const nameInput = document.getElementById('hero-name');
        const name = nameInput.value.trim();
        
        if (!name) {
            nameInput.focus();
            return;
        }
        
        const selectedClass = document.querySelector('.class-card.selected');
        if (!selectedClass) {
            return;
        }
        
        const classType = selectedClass.dataset.class;
        const charData = CHARACTER_CLASSES[classType];
        
        this.state.character = {
            class: classType,
            name: name,
            stats: { ...charData.stats }
        };
        
        audio.play('buttonClick');
        this.save();
        this.render();
        
        // Welcome message
        effects.triggerCelebration('achievement');
    }

    /**
     * Toggle sound on/off
     */
    toggleSound() {
        audio.init();
        audio.enabled = audio.toggle();
        
        document.getElementById('sound-toggle').textContent = audio.enabled ? '🔊' : '🔇';
        this.state.settings.soundEnabled = audio.enabled;
        
        if (audio.enabled) {
            audio.resume();
            audio.play('buttonClick');
        }
        
        this.save();
    }

    /**
     * Add a new task
     */
    addTask() {
        const input = document.getElementById('task-input');
        const difficultySelect = document.getElementById('difficulty-select');
        
        const text = input.value.trim();
        if (!text) return;
        
        const difficulty = difficultySelect.value;
        
        // Check if epic is allowed
        if (difficulty === 'epic' && this.state.xp.level < 10) {
            alert('Epic tasks are only available at Level 10!');
            return;
        }
        
        const task = {
            id: Date.now().toString(),
            text: text,
            difficulty: difficulty,
            completed: false,
            createdAt: new Date().toISOString()
        };
        
        this.state.tasks.unshift(task);
        this.save();
        this.render();
        
        audio.play('buttonClick');
        input.value = '';
    }

    /**
     * Toggle task completion
     */
    toggleTask(taskId) {
        const task = this.state.tasks.find(t => t.id === taskId);
        if (!task) return;
        
        if (task.completed) {
            // Uncomplete - remove XP and revert streak
            task.completed = false;
            task.completedAt = null;
            
            // Note: For simplicity, we don't remove XP already earned
            // In a more complex system, you'd track this
        } else {
            // Complete task
            task.completed = true;
            task.completedAt = new Date().toISOString();
            
            // Calculate XP
            const xpGained = this.calculateXP(task);
            
            // Get task element for effects
            const taskElement = document.querySelector(`[data-task-id="${taskId}"]`);
            
            // Apply XP
            const oldLevel = this.state.xp.level;
            this.state.xp.current += xpGained;
            this.state.xp.total += xpGained;
            
            // Check for level up
            const newLevel = this.calculateLevel();
            
            // Update streak
            this.updateStreak();
            
            // Show effects
            if (taskElement) {
                effects.createXPText(xpGained, taskElement);
                effects.triggerCelebration('taskComplete');
            }
            
            // Play sound
            audio.play('taskComplete');
            
            // Check level up
            if (newLevel > oldLevel) {
                this.handleLevelUp(newLevel);
            }
        }
        
        this.save();
        this.render();
    }

    /**
     * Delete a task
     */
    deleteTask(taskId) {
        this.state.tasks = this.state.tasks.filter(t => t.id !== taskId);
        this.save();
        this.render();
        audio.play('buttonClick');
    }

    /**
     * Calculate XP for a task
     */
    calculateXP(task) {
        let xp = BASE_XP[task.difficulty];
        const char = this.state.character;
        const streak = this.state.streak;
        
        // Class bonuses
        if (char.class === 'warrior' && (task.difficulty === 'hard' || task.difficulty === 'epic')) {
            xp *= 1.2; // +20% XP
        }
        
        if (char.class === 'mage') {
            xp += char.stats.int; // +INT bonus
        }
        
        // Calculate streak multiplier with class bonus
        let streakMultiplier = streak.multiplier;
        if (char.class === 'rogue') {
            streakMultiplier *= 1.15; // +15% for rogue
        }
        
        // Check for perks
        if (this.state.perks.includes('habit_architect')) {
            streakMultiplier *= 1.1; // +10% from perk
        }
        
        // Check for power hour (first task of the day)
        const todayStr = new Date().toDateString();
        const todayTasks = this.state.tasks.filter(t => 
            t.completed && new Date(t.completedAt).toDateString() === todayStr
        );
        
        if (this.state.perks.includes('power_hour') && todayTasks.length === 0) {
            xp *= 2; // Double XP for first task
        }
        
        // Apply streak multiplier
        xp *= streakMultiplier;
        
        // Level bonus (+5% per level, from perk)
        if (this.state.perks.includes('quick_learner')) {
            xp *= (1 + (this.state.xp.level - 1) * 0.05);
        }
        
        return Math.floor(xp);
    }

    /**
     * Calculate current level from total XP
     */
    calculateLevel() {
        let level = 1;
        for (let i = 1; i < LEVEL_THRESHOLDS.length; i++) {
            if (this.state.xp.total >= LEVEL_THRESHOLDS[i]) {
                level = i + 1;
            } else {
                break;
            }
        }
        return Math.min(level, 10);
    }

    /**
     * Get XP needed for next level
     */
    getXPForNextLevel() {
        const currentLevel = this.state.xp.level;
        if (currentLevel >= 10) return null;
        return LEVEL_THRESHOLDS[currentLevel] - this.state.xp.total;
    }

    /**
     * Get XP progress in current level
     */
    getXPProgress() {
        const currentLevel = this.state.xp.level;
        const currentThreshold = LEVEL_THRESHOLDS[currentLevel - 1] || 0;
        const nextThreshold = LEVEL_THRESHOLDS[currentLevel] || LEVEL_THRESHOLDS[10];
        
        const xpInLevel = this.state.xp.total - currentThreshold;
        const xpNeeded = nextThreshold - currentThreshold;
        
        return {
            current: xpInLevel,
            needed: xpNeeded,
            percentage: Math.min((xpInLevel / xpNeeded) * 100, 100)
        };
    }

    /**
     * Handle level up
     */
    handleLevelUp(newLevel) {
        this.state.xp.level = newLevel;
        
        // Show celebration
        const overlay = document.getElementById('celebration-overlay');
        document.getElementById('celebration-title').textContent = `🎉 Level Up! 🎉`;
        document.getElementById('celebration-message').textContent = `You reached Level ${newLevel}!`;
        
        // Check for new perk
        const perkDiv = document.getElementById('celebration-perk');
        const newPerk = Object.entries(PERKS).find(([id, p]) => 
            p.level === newLevel && !this.state.perks.includes(id)
        );
        
        if (newPerk) {
            const [perkId, perkData] = newPerk;
            this.state.perks.push(perkId);
            
            document.getElementById('perk-name').textContent = `${perkData.icon} ${perkData.name}`;
            perkDiv.classList.remove('hidden');
            
            // Unlock epic tasks at level 10
            if (perkId === 'master_achiever') {
                document.getElementById('epic-option').disabled = false;
            }
        } else {
            perkDiv.classList.add('hidden');
        }
        
        overlay.classList.remove('hidden');
        
        // Effects
        effects.triggerCelebration('levelUp');
        effects.shakeScreen();
        audio.play('levelUp');
        
        this.save();
    }

    /**
     * Update streak
     */
    updateStreak() {
        const today = new Date().toDateString();
        const lastDate = this.state.streak.lastCompletedDate;
        
        // Check if already completed today
        if (lastDate === today) return;
        
        const yesterday = new Date();
        yesterday.setDate(yesterday.getDate() - 1);
        
        if (lastDate === yesterday.toDateString()) {
            // Consecutive day - increment streak
            this.state.streak.current++;
        } else if (lastDate !== today) {
            // Streak broken or first day
            this.state.streak.current = 1;
        }
        
        // Update longest
        if (this.state.streak.current > this.state.streak.longest) {
            this.state.streak.longest = this.state.streak.current;
        }
        
        // Calculate multiplier
        const streak = this.state.streak.current;
        if (streak >= 30) {
            this.state.streak.multiplier = 3.0;
            if (streak === 30) audio.play('streak30');
        } else if (streak >= 14) {
            this.state.streak.multiplier = 2.0;
            if (streak === 14) audio.play('streak14');
        } else if (streak >= 7) {
            this.state.streak.multiplier = 1.5;
            if (streak === 7) audio.play('streak7');
        } else {
            this.state.streak.multiplier = 1.0;
        }
        
        this.state.streak.lastCompletedDate = today;
        
        // Effects for milestones
        if ([7, 14, 30].includes(streak)) {
            effects.triggerCelebration('streak');
        }
    }

    /**
     * Check for new day (reset daily bonuses)
     */
    checkNewDay() {
        const today = new Date().toDateString();
        const lastDate = this.state.streak.lastCompletedDate;
        
        // If last completion was before yesterday, streak is potentially broken
        // (We'll only reset if no task is completed today)
        if (lastDate && lastDate !== today) {
            const yesterday = new Date();
            yesterday.setDate(yesterday.getDate() - 1);
            
            if (lastDate !== yesterday.toDateString()) {
                // Streak is broken
                this.state.streak.current = 0;
                this.state.streak.multiplier = 1.0;
            }
        }
    }

    /**
     * Save game state
     */
    save() {
        storage.save(this.state);
    }

    /**
     * Start auto-save timer
     */
    startAutoSave() {
        this.autoSaveInterval = setInterval(() => {
            this.save();
        }, 30000); // Every 30 seconds
    }

    /**
     * Render the UI
     */
    render() {
        // Show/hide character selection
        const charSelect = document.getElementById('character-select');
        const gameArea = document.getElementById('game-area');
        
        if (!this.state.character) {
            charSelect.classList.remove('hidden');
            gameArea.classList.add('hidden');
            return;
        }
        
        charSelect.classList.add('hidden');
        gameArea.classList.remove('hidden');
        
        // Character info
        const charData = CHARACTER_CLASSES[this.state.character.class];
        document.getElementById('char-icon').textContent = charData.icon;
        document.getElementById('char-name').textContent = this.state.character.name;
        document.getElementById('char-class').textContent = 
            `${charData.name} Lv.${this.state.xp.level}`;
        
        // XP bar
        const progress = this.getXPProgress();
        document.getElementById('xp-current').textContent = 
            `${progress.current} / ${progress.needed}`;
        document.getElementById('xp-fill').style.width = `${progress.percentage}%`;
        document.getElementById('current-level').textContent = this.state.xp.level;
        
        // Streak
        document.getElementById('streak-count').textContent = this.state.streak.current;
        document.getElementById('streak-multiplier').textContent = 
            `${this.state.streak.multiplier}x`;
        
        // Task list
        this.renderTasks();
        
        // Perks
        this.renderPerks();
    }

    /**
     * Render task list
     */
    renderTasks() {
        const taskList = document.getElementById('task-list');
        const emptyState = document.getElementById('empty-state');
        
        if (this.state.tasks.length === 0) {
            taskList.innerHTML = '';
            emptyState.classList.remove('hidden');
            return;
        }
        
        emptyState.classList.add('hidden');
        
        taskList.innerHTML = this.state.tasks.map(task => `
            <div class="task-item difficulty-${task.difficulty} ${task.completed ? 'completed' : ''}" 
                 data-task-id="${task.id}">
                <input type="checkbox" class="task-checkbox" 
                       ${task.completed ? 'checked' : ''} 
                       onchange="game.toggleTask('${task.id}')">
                <div class="task-content">
                    <div class="task-text">${this.escapeHtml(task.text)}</div>
                    <span class="task-difficulty ${task.difficulty}">
                        ${task.difficulty.toUpperCase()} +${BASE_XP[task.difficulty]} XP
                    </span>
                </div>
                <button class="task-delete" onclick="game.deleteTask('${task.id}')" title="Delete">✕</button>
            </div>
        `).join('');
    }

    /**
     * Render perks
     */
    renderPerks() {
        const perksList = document.getElementById('perks-list');
        
        perksList.innerHTML = Object.entries(PERKS).map(([id, perk]) => {
            const unlocked = this.state.perks.includes(id);
            const levelReached = this.state.xp.level >= perk.level;
            
            return `
                <div class="perk-item ${unlocked ? 'unlocked' : 'locked'}">
                    <span class="perk-icon">${unlocked ? perk.icon : '🔒'}</span>
                    <span class="perk-name">${unlocked ? perk.name : '???'}</span>
                    <span class="perk-level">Lv.${perk.level}</span>
                </div>
            `;
        }).join('');
    }

    /**
     * Escape HTML to prevent XSS
     */
    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
}

// ============================================
// INITIALIZE
// ============================================

let game;

document.addEventListener('DOMContentLoaded', () => {
    game = new QuestDoGame();
    game.init();
});
