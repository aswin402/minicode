/**
 * QuestDo Storage Manager
 * Handles localStorage persistence with versioning and migration
 */

class StorageManager {
    constructor() {
        this.SAVE_KEY = 'questdo_save';
        this.SAVE_VERSION = '1.0';
    }

    /**
     * Get default game state
     */
    getDefaultState() {
        return {
            version: this.SAVE_VERSION,
            character: null,
            xp: {
                current: 0,
                total: 0,
                level: 1
            },
            streak: {
                current: 0,
                longest: 0,
                lastCompletedDate: null,
                multiplier: 1.0
            },
            perks: [],
            tasks: [],
            settings: {
                soundEnabled: true
            },
            lastSave: new Date().toISOString()
        };
    }

    /**
     * Save game state to localStorage
     */
    save(state) {
        try {
            const saveData = {
                ...state,
                version: this.SAVE_VERSION,
                lastSave: new Date().toISOString()
            };
            
            const serialized = JSON.stringify(saveData);
            
            // Check size limit (5MB localStorage limit, use 4.5MB for safety)
            if (serialized.length > 4.5 * 1024 * 1024) {
                console.warn('Save data too large, pruning old tasks...');
                return this.savePruned(state);
            }
            
            localStorage.setItem(this.SAVE_KEY, serialized);
            return true;
        } catch (e) {
            console.error('Failed to save game:', e);
            return false;
        }
    }

    /**
     * Save with pruned old completed tasks
     */
    savePruned(state) {
        try {
            // Keep only last 50 completed tasks
            const completedTasks = state.tasks
                .filter(t => t.completed)
                .slice(-50);
            const incompleteTasks = state.tasks.filter(t => !t.completed);
            
            const prunedState = {
                ...state,
                tasks: [...incompleteTasks, ...completedTasks],
                version: this.SAVE_VERSION,
                lastSave: new Date().toISOString()
            };
            
            localStorage.setItem(this.SAVE_KEY, JSON.stringify(prunedState));
            return true;
        } catch (e) {
            console.error('Failed to save pruned data:', e);
            return false;
        }
    }

    /**
     * Load game state from localStorage
     */
    load() {
        try {
            const saved = localStorage.getItem(this.SAVE_KEY);
            
            if (!saved) {
                return this.getDefaultState();
            }
            
            const state = JSON.parse(saved);
            
            // Migrate if needed
            if (this.needsMigration(state)) {
                return this.migrate(state);
            }
            
            return this.validateState(state);
        } catch (e) {
            console.error('Failed to load game:', e);
            return this.getDefaultState();
        }
    }

    /**
     * Check if state needs migration
     */
    needsMigration(state) {
        return !state.version || state.version !== this.SAVE_VERSION;
    }

    /**
     * Migrate old save data to current version
     */
    migrate(state) {
        console.log('Migrating save data from version:', state.version || 'unknown');
        
        const defaultState = this.getDefaultState();
        
        // Merge with defaults for any missing fields
        return {
            ...defaultState,
            ...state,
            version: this.SAVE_VERSION,
            // Ensure nested objects exist
            xp: { ...defaultState.xp, ...state.xp },
            streak: { ...defaultState.streak, ...state.streak },
            settings: { ...defaultState.settings, ...state.settings }
        };
    }

    /**
     * Validate and sanitize loaded state
     */
    validateState(state) {
        const defaultState = this.getDefaultState();
        
        // Ensure all required fields exist
        const validated = {
            version: state.version || defaultState.version,
            character: state.character || null,
            xp: {
                current: Number.isFinite(state.xp?.current) ? state.xp.current : defaultState.xp.current,
                total: Number.isFinite(state.xp?.total) ? state.xp.total : defaultState.xp.total,
                level: Number.isFinite(state.xp?.level) ? Math.max(1, state.xp.level) : defaultState.xp.level
            },
            streak: {
                current: Number.isFinite(state.streak?.current) ? state.streak.current : defaultState.streak.current,
                longest: Number.isFinite(state.streak?.longest) ? state.streak.longest : defaultState.streak.longest,
                lastCompletedDate: state.streak?.lastCompletedDate || null,
                multiplier: Number.isFinite(state.streak?.multiplier) ? state.streak.multiplier : defaultState.streak.multiplier
            },
            perks: Array.isArray(state.perks) ? state.perks : defaultState.perks,
            tasks: Array.isArray(state.tasks) ? state.tasks : defaultState.tasks,
            settings: {
                soundEnabled: typeof state.settings?.soundEnabled === 'boolean' ? state.settings.soundEnabled : true
            },
            lastSave: state.lastSave || new Date().toISOString()
        };
        
        return validated;
    }

    /**
     * Clear all save data
     */
    clear() {
        try {
            localStorage.removeItem(this.SAVE_KEY);
            return true;
        } catch (e) {
            console.error('Failed to clear save:', e);
            return false;
        }
    }

    /**
     * Export save data as JSON string
     */
    export() {
        try {
            const saved = localStorage.getItem(this.SAVE_KEY);
            if (!saved) return null;
            
            const blob = new Blob([saved], { type: 'application/json' });
            const url = URL.createObjectURL(blob);
            
            const a = document.createElement('a');
            a.href = url;
            a.download = `questdo-save-${new Date().toISOString().split('T')[0]}.json`;
            document.body.appendChild(a);
            a.click();
            document.body.removeChild(a);
            URL.revokeObjectURL(url);
            
            return true;
        } catch (e) {
            console.error('Failed to export save:', e);
            return false;
        }
    }

    /**
     * Import save data from JSON string
     */
    import(jsonString) {
        try {
            const data = JSON.parse(jsonString);
            const validated = this.validateState(data);
            return this.save(validated);
        } catch (e) {
            console.error('Failed to import save:', e);
            return false;
        }
    }
}

// Export singleton instance
const storage = new StorageManager();
