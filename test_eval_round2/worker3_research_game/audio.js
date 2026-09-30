/**
 * Todo Quest — Audio Engine
 * Procedural sound generation via Web Audio API.
 * No external audio files required.
 */
class AudioEngine {
  constructor() {
    this._ctx = null;
    this._initialized = false;
  }

  /** Lazily create and resume AudioContext on first user gesture. */
  _ensureContext() {
    if (!this._ctx) {
      this._ctx = new (window.AudioContext || window.webkitAudioContext)();
    }
    if (this._ctx.state === 'suspended') {
      this._ctx.resume();
    }
    this._initialized = true;
  }

  /**
   * Create an oscillator node with envelope shaping.
   * @param {string} type - Oscillator type: 'sine', 'triangle', 'square', 'sawtooth'
   * @param {number} frequency - Starting frequency in Hz
   * @param {number} startTime - When to start (seconds from now)
   * @param {number} duration - Total duration (seconds)
   * @param {number} peakGain - Peak volume (0-1)
   * @param {number} [freqEnd] - Ending frequency for slide (optional)
   */
  _tone(type, frequency, startTime, duration, peakGain, freqEnd = null) {
    const ctx = this._ctx;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();

    osc.type = type;
    osc.frequency.setValueAtTime(frequency, ctx.currentTime + startTime);
    if (freqEnd !== null) {
      osc.frequency.linearRampToValueAtTime(freqEnd, ctx.currentTime + startTime + duration);
    }

    // ADSR-style envelope: quick attack, sustain, quick release
    const t0 = ctx.currentTime + startTime;
    const attack  = 0.02;
    const release = Math.min(0.15, duration * 0.4);

    gain.gain.setValueAtTime(0, t0);
    gain.gain.linearRampToValueAtTime(peakGain, t0 + attack);
    gain.gain.setValueAtTime(peakGain, t0 + duration - release);
    gain.gain.linearRampToValueAtTime(0, t0 + duration);

    osc.connect(gain);
    gain.connect(ctx.destination);

    osc.start(t0);
    osc.stop(t0 + duration + 0.05);
  }

  /**
   * Play a single note.
   * @param {number} freq - Frequency in Hz
   * @param {string} type - Oscillator type
   * @param {number} startTime - Seconds from now
   * @param {number} duration
   * @param {number} gain
   */
  _note(freq, type, startTime, duration, gain) {
    this._tone(type, freq, startTime, duration, gain);
  }

  /**
   * Quest completion chime: rising 3-note arpeggio (C5-E5-G5).
   */
  playChime() {
    this._ensureContext();
    const C5 = 523.25, E5 = 659.25, G5 = 783.99;
    this._note(C5, 'sine',    0.00, 0.18, 0.30);
    this._note(E5, 'sine',    0.10, 0.18, 0.30);
    this._note(G5, 'sine',    0.20, 0.25, 0.28);
    // Harmonics for richness
    this._note(G5 * 2, 'sine', 0.20, 0.15, 0.08);
    // Final shimmer
    this._note(C5 * 2, 'sine', 0.30, 0.30, 0.05);
  }

  /**
   * Level-up fanfare: triumphant ascending melody.
   */
  playLevelUp() {
    this._ensureContext();
    // C4 E4 G4 C5 — ascending triumphant arpeggio
    const notes = [261.63, 329.63, 392.00, 523.25, 659.25];
    const spacing = 0.13;
    notes.forEach((freq, i) => {
      const dur = i === notes.length - 1 ? 0.6 : 0.25;
      this._note(freq, 'triangle', i * spacing, dur, 0.28);
      this._note(freq * 2, 'sine', i * spacing, dur * 0.5, 0.06);
    });
    // Closing sustained chord
    const chordTime = notes.length * spacing;
    [523.25, 659.25, 783.99].forEach((freq, i) => {
      this._note(freq, 'sine', chordTime, 0.8, 0.12);
    });
  }

  /**
   * Streak milestone celebration: regal descending-then-ascending fanfare.
   */
  playStreakMilestone() {
    this._ensureContext();
    // Descending: G5 F5 E5 D5, then ascending: E5 F5 G5
    const desc = [783.99, 698.46, 659.25, 587.33];
    const asc  = [659.25, 698.46, 783.99, 1046.50];
    desc.forEach((freq, i) => {
      this._note(freq, 'triangle', i * 0.12, 0.2, 0.25);
    });
    asc.forEach((freq, i) => {
      this._note(freq, 'triangle', (desc.length + i) * 0.12, 0.3, 0.28);
      this._note(freq * 2, 'sine', (desc.length + i) * 0.12, 0.15, 0.05);
    });
  }

  /**
   * Streak-break notification: subtle descending tone (not punishing).
   */
  playStreakBreak() {
    this._ensureContext();
    const notes = [440, 392, 349.23];
    notes.forEach((freq, i) => {
      this._note(freq, 'sine', i * 0.15, 0.3, 0.15);
    });
  }

  /**
   * Button/interaction click: short tick.
   */
  playClick() {
    this._ensureContext();
    this._note(880, 'sine', 0, 0.05, 0.08);
  }

  /**
   * XP counter tick: very quiet, short blip (used for number animation).
   */
  playTick() {
    this._ensureContext();
    this._note(1200, 'sine', 0, 0.03, 0.03);
  }

  /**
   * Unlock badge: sparkle arpeggio.
   */
  playBadgeUnlock() {
    this._ensureContext();
    const notes = [1046.50, 1318.51, 1567.98];
    notes.forEach((freq, i) => {
      this._note(freq, 'sine', i * 0.08, 0.2, 0.18);
      this._note(freq * 2, 'sine', i * 0.08, 0.1, 0.04);
    });
  }
}

// Export singleton instance
const audio = new AudioEngine();
