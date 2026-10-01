/**
 * QuestDo — Procedural Audio Engine
 * Web Audio API synthesis: zero external assets
 * Safe: all functions guarded against suspended AudioContext
 */

const AudioEngine = (() => {
  let ctx = null;
  let muted = false;

  const init = () => {
    if (!ctx) {
      try {
        ctx = new (window.AudioContext || window.webkitAudioContext)();
      } catch (e) { return null; }
    }
    if (ctx && ctx.state === 'suspended') {
      ctx.resume().catch(() => {});
    }
    return ctx;
  };

  const getCtx = () => {
    if (!ctx) init();
    return ctx;
  };

  const setMuted = (val) => { muted = val; };
  const isMuted = () => muted;

  // Guard: skip if context not running
  const safePlay = (fn) => {
    if (muted) return;
    try {
      if (!ctx || ctx.state !== 'running') {
        if (ctx && ctx.state === 'suspended') ctx.resume().catch(() => {});
        return;
      }
      fn();
    } catch (e) { /* silent audio fail */ }
  };

  // --- Task Complete: Triangle sweep 440→880Hz, 150ms ---
  const playComplete = () => {
    safePlay(() => {
      const c = ctx;
      const t = c.currentTime;
      const osc = c.createOscillator();
      const g = c.createGain();
      osc.type = 'triangle';
      osc.frequency.setValueAtTime(440, t);
      osc.frequency.exponentialRampToValueAtTime(880, t + 0.12);
      g.gain.setValueAtTime(0.35, t);
      g.gain.exponentialRampToValueAtTime(0.001, t + 0.18);
      osc.connect(g);
      g.connect(c.destination);
      osc.start(t);
      osc.stop(t + 0.2);
    });
  };

  // --- Level Up: Square fanfare C4→E4→G4→C5 arpeggio ---
  const playLevelUp = () => {
    safePlay(() => {
      const c = ctx;
      const notes = [261.63, 329.63, 392.00, 523.25];
      const duration = 0.15;
      notes.forEach((freq, i) => {
        const t = c.currentTime + i * duration;
        const osc = c.createOscillator();
        const g = c.createGain();
        osc.type = 'square';
        osc.frequency.setValueAtTime(freq, t);
        g.gain.setValueAtTime(0, t);
        g.gain.linearRampToValueAtTime(0.25, t + 0.02);
        g.gain.setValueAtTime(0.25, t + duration - 0.04);
        g.gain.exponentialRampToValueAtTime(0.001, t + duration + 0.05);
        osc.connect(g);
        g.connect(c.destination);
        osc.start(t);
        osc.stop(t + duration + 0.1);
      });
    });
  };

  // --- Streak Bonus: Sine ascending 523→659→784Hz ---
  const playStreakBonus = () => {
    safePlay(() => {
      const c = ctx;
      const notes = [523.25, 659.25, 783.99];
      const duration = 0.1;
      notes.forEach((freq, i) => {
        const t = c.currentTime + i * duration;
        const osc = c.createOscillator();
        const g = c.createGain();
        osc.type = 'sine';
        osc.frequency.setValueAtTime(freq, t);
        g.gain.setValueAtTime(0.3, t);
        g.gain.exponentialRampToValueAtTime(0.001, t + duration + 0.05);
        osc.connect(g);
        g.connect(c.destination);
        osc.start(t);
        osc.stop(t + duration + 0.1);
      });
    });
  };

  // --- Error: Low square 110Hz, 100ms ---
  const playError = () => {
    safePlay(() => {
      const c = ctx;
      const t = c.currentTime;
      const osc = c.createOscillator();
      const g = c.createGain();
      osc.type = 'square';
      osc.frequency.setValueAtTime(110, t);
      g.gain.setValueAtTime(0.3, t);
      g.gain.exponentialRampToValueAtTime(0.001, t + 0.12);
      osc.connect(g);
      g.connect(c.destination);
      osc.start(t);
      osc.stop(t + 0.15);
    });
  };

  // --- Daily Reset: Sawtooth descend 220→55Hz ---
  const playDailyReset = () => {
    safePlay(() => {
      const c = ctx;
      const t = c.currentTime;
      const osc = c.createOscillator();
      const g = c.createGain();
      osc.type = 'sawtooth';
      osc.frequency.setValueAtTime(220, t);
      osc.frequency.exponentialRampToValueAtTime(55, t + 0.22);
      g.gain.setValueAtTime(0.2, t);
      g.gain.exponentialRampToValueAtTime(0.001, t + 0.25);
      osc.connect(g);
      g.connect(c.destination);
      osc.start(t);
      osc.stop(t + 0.3);
    });
  };

  return { init, setMuted, isMuted, playComplete, playLevelUp, playStreakBonus, playError, playDailyReset };
})();

// Auto-init on any user gesture
['click', 'touchstart', 'keydown'].forEach(ev => {
  document.addEventListener(ev, () => AudioEngine.init(), { once: true });
});
