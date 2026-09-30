/**
 * QuestDo Audio Engine - Procedural Web Audio Synthesis
 * No external audio files required - all sounds generated in real-time
 */

class AudioEngine {
    constructor() {
        this.context = null;
        this.enabled = true;
        this.masterGain = null;
    }

    /**
     * Initialize AudioContext (must be called after user gesture)
     */
    init() {
        if (this.context) return;
        
        try {
            this.context = new (window.AudioContext || window.webkitAudioContext)();
            this.masterGain = this.context.createGain();
            this.masterGain.gain.value = 0.3;
            this.masterGain.connect(this.context.destination);
        } catch (e) {
            console.warn('Web Audio API not supported:', e);
            this.enabled = false;
        }
    }

    /**
     * Resume AudioContext if suspended (required by browsers)
     */
    async resume() {
        if (this.context && this.context.state === 'suspended') {
            await this.context.resume();
        }
    }

    /**
     * Toggle sound on/off
     */
    toggle() {
        this.enabled = !this.enabled;
        return this.enabled;
    }

    /**
     * Play a procedural sound effect
     */
    play(type) {
        if (!this.enabled || !this.context) return;
        
        try {
            switch (type) {
                case 'taskComplete':
                    this.playTaskComplete();
                    break;
                case 'levelUp':
                    this.playLevelUp();
                    break;
                case 'streak7':
                    this.playStreakMilestone(7);
                    break;
                case 'streak14':
                    this.playStreakMilestone(14);
                    break;
                case 'streak30':
                    this.playStreakMilestone(30);
                    break;
                case 'achievement':
                    this.playAchievement();
                    break;
                case 'buttonClick':
                    this.playButtonClick();
                    break;
                default:
                    console.warn('Unknown sound type:', type);
            }
        } catch (e) {
            console.warn('Audio playback error:', e);
        }
    }

    /**
     * Task completion sound - bright ascending chime
     */
    playTaskComplete() {
        const ctx = this.context;
        const now = ctx.currentTime;
        
        // Main tone
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        
        osc.type = 'sine';
        osc.frequency.setValueAtTime(523.25, now); // C5
        osc.frequency.exponentialRampToValueAtTime(1046.5, now + 0.15); // C6
        
        gain.gain.setValueAtTime(0.3, now);
        gain.gain.exponentialRampToValueAtTime(0.001, now + 0.2);
        
        osc.connect(gain);
        gain.connect(this.masterGain);
        
        osc.start(now);
        osc.stop(now + 0.2);
        
        // Harmony
        const osc2 = ctx.createOscillator();
        const gain2 = ctx.createGain();
        
        osc2.type = 'sine';
        osc2.frequency.setValueAtTime(659.25, now + 0.05); // E5
        osc2.frequency.exponentialRampToValueAtTime(1318.5, now + 0.2);
        
        gain2.gain.setValueAtTime(0, now);
        gain2.gain.linearRampToValueAtTime(0.2, now + 0.05);
        gain2.gain.exponentialRampToValueAtTime(0.001, now + 0.25);
        
        osc2.connect(gain2);
        gain2.connect(this.masterGain);
        
        osc2.start(now);
        osc2.stop(now + 0.25);
    }

    /**
     * Level up sound - triumphant major chord arpeggio
     */
    playLevelUp() {
        const ctx = this.context;
        const now = ctx.currentTime;
        
        // C Major chord: C4, E4, G4
        const frequencies = [261.63, 329.63, 392.00, 523.25]; // C4, E4, G4, C5
        
        frequencies.forEach((freq, i) => {
            const osc = ctx.createOscillator();
            const gain = ctx.createGain();
            
            osc.type = 'triangle';
            osc.frequency.value = freq;
            
            const startTime = now + (i * 0.1);
            gain.gain.setValueAtTime(0, startTime);
            gain.gain.linearRampToValueAtTime(0.25, startTime + 0.05);
            gain.gain.exponentialRampToValueAtTime(0.001, startTime + 0.6);
            
            osc.connect(gain);
            gain.connect(this.masterGain);
            
            osc.start(startTime);
            osc.stop(startTime + 0.6);
        });
        
        // Final flourish
        const finalOsc = ctx.createOscillator();
        const finalGain = ctx.createGain();
        
        finalOsc.type = 'sine';
        finalOsc.frequency.setValueAtTime(1046.5, now + 0.4);
        finalOsc.frequency.exponentialRampToValueAtTime(2093, now + 0.8);
        
        finalGain.gain.setValueAtTime(0.2, now + 0.4);
        finalGain.gain.exponentialRampToValueAtTime(0.001, now + 1);
        
        finalOsc.connect(finalGain);
        finalGain.connect(this.masterGain);
        
        finalOsc.start(now + 0.4);
        finalOsc.stop(now + 1);
    }

    /**
     * Streak milestone sound - victory burst with filter sweep
     */
    playStreakMilestone(days) {
        const ctx = this.context;
        const now = ctx.currentTime;
        
        // Intensity based on streak length
        const intensity = Math.min(days / 30, 1);
        const baseFreq = 330 + (intensity * 220);
        
        // Sawtooth for richness
        const osc1 = ctx.createOscillator();
        const gain1 = ctx.createGain();
        const filter1 = ctx.createBiquadFilter();
        
        osc1.type = 'sawtooth';
        osc1.frequency.setValueAtTime(baseFreq, now);
        osc1.frequency.exponentialRampToValueAtTime(baseFreq * 2, now + 0.3);
        
        filter1.type = 'lowpass';
        filter1.frequency.setValueAtTime(2000 + (intensity * 3000), now);
        filter1.frequency.exponentialRampToValueAtTime(200, now + 0.4);
        
        gain1.gain.setValueAtTime(0.2, now);
        gain1.gain.exponentialRampToValueAtTime(0.001, now + 0.4);
        
        osc1.connect(filter1);
        filter1.connect(gain1);
        gain1.connect(this.masterGain);
        
        osc1.start(now);
        osc1.stop(now + 0.4);
        
        // Square wave accent
        const osc2 = ctx.createOscillator();
        const gain2 = ctx.createGain();
        
        osc2.type = 'square';
        osc2.frequency.setValueAtTime(baseFreq * 1.5, now);
        osc2.frequency.setValueAtTime(baseFreq * 2, now + 0.1);
        
        gain2.gain.setValueAtTime(0.1, now);
        gain2.gain.exponentialRampToValueAtTime(0.001, now + 0.3);
        
        osc2.connect(gain2);
        gain2.connect(this.masterGain);
        
        osc2.start(now);
        osc2.stop(now + 0.3);
    }

    /**
     * Achievement unlock sound - crystal reverb ding
     */
    playAchievement() {
        const ctx = this.context;
        const now = ctx.currentTime;
        
        // High crystal tone
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        const filter = ctx.createBiquadFilter();
        
        osc.type = 'sine';
        osc.frequency.value = 2093; // C7
        
        // Simple reverb simulation with delay
        const delay = ctx.createDelay();
        const delayGain = ctx.createGain();
        
        delay.delayTime.value = 0.1;
        delayGain.gain.value = 0.3;
        
        filter.type = 'highpass';
        filter.frequency.value = 1500;
        
        gain.gain.setValueAtTime(0.25, now);
        gain.gain.exponentialRampToValueAtTime(0.001, now + 0.8);
        
        osc.connect(gain);
        gain.connect(filter);
        filter.connect(this.masterGain);
        
        // Delay for reverb effect
        filter.connect(delay);
        delay.connect(delayGain);
        delayGain.connect(this.masterGain);
        
        osc.start(now);
        osc.stop(now + 0.8);
        
        // Secondary shimmer
        const osc2 = ctx.createOscillator();
        const gain2 = ctx.createGain();
        
        osc2.type = 'triangle';
        osc2.frequency.setValueAtTime(2637, now + 0.1); // E7
        osc2.frequency.exponentialRampToValueAtTime(3520, now + 0.4); // A7
        
        gain2.gain.setValueAtTime(0, now);
        gain2.gain.linearRampToValueAtTime(0.15, now + 0.15);
        gain2.gain.exponentialRampToValueAtTime(0.001, now + 0.6);
        
        osc2.connect(gain2);
        gain2.connect(this.masterGain);
        
        osc2.start(now + 0.1);
        osc2.stop(now + 0.6);
    }

    /**
     * Button click sound - subtle UI feedback
     */
    playButtonClick() {
        const ctx = this.context;
        const now = ctx.currentTime;
        
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        
        osc.type = 'sine';
        osc.frequency.setValueAtTime(800, now);
        osc.frequency.linearRampToValueAtTime(400, now + 0.05);
        
        gain.gain.setValueAtTime(0.1, now);
        gain.gain.exponentialRampToValueAtTime(0.001, now + 0.05);
        
        osc.connect(gain);
        gain.connect(this.masterGain);
        
        osc.start(now);
        osc.stop(now + 0.05);
    }
}

// Export singleton instance
const audio = new AudioEngine();
