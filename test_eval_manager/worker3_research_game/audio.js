/**
 * Todo Quest - Audio Module
 * Web Audio API sound generation (no external files)
 */

let audioContext = null;

function getAudioContext() {
  if (!audioContext) {
    audioContext = new (window.AudioContext || window.webkitAudioContext)();
  }
  return audioContext;
}

// Resume audio context on user interaction (required by browsers)
export function initAudio() {
  const ctx = getAudioContext();
  if (ctx.state === 'suspended') {
    ctx.resume();
  }
}

/**
 * Play a completion "ding" sound
 * Pleasant upward frequency jump
 */
export function playCompletionSound() {
  const ctx = getAudioContext();
  const oscillator = ctx.createOscillator();
  const gainNode = ctx.createGain();
  
  oscillator.connect(gainNode);
  gainNode.connect(ctx.destination);
  
  oscillator.type = 'sine';
  oscillator.frequency.setValueAtTime(880, ctx.currentTime);
  oscillator.frequency.exponentialRampToValueAtTime(1100, ctx.currentTime + 0.15);
  
  gainNode.gain.setValueAtTime(0.3, ctx.currentTime);
  gainNode.gain.exponentialRampToValueAtTime(0.01, ctx.currentTime + 0.2);
  
  oscillator.start(ctx.currentTime);
  oscillator.stop(ctx.currentTime + 0.2);
}

/**
 * Play level up fanfare
 * Triumphant 3-note ascending chord
 */
export function playLevelUpSound() {
  const ctx = getAudioContext();
  const notes = [523.25, 659.25, 783.99]; // C5, E5, G5
  
  notes.forEach((freq, index) => {
    const oscillator = ctx.createOscillator();
    const gainNode = ctx.createGain();
    
    oscillator.connect(gainNode);
    gainNode.connect(ctx.destination);
    
    oscillator.type = 'triangle';
    oscillator.frequency.setValueAtTime(freq, ctx.currentTime);
    
    const startTime = ctx.currentTime + (index * 0.12);
    gainNode.gain.setValueAtTime(0, startTime);
    gainNode.gain.linearRampToValueAtTime(0.25, startTime + 0.05);
    gainNode.gain.exponentialRampToValueAtTime(0.01, startTime + 0.5);
    
    oscillator.start(startTime);
    oscillator.stop(startTime + 0.5);
  });
}

/**
 * Play streak milestone sound
 * Bright ascending arpeggio with sparkle
 */
export function playStreakMilestoneSound() {
  const ctx = getAudioContext();
  const notes = [523.25, 659.25, 783.99, 1046.5]; // C5, E5, G5, C6
  
  notes.forEach((freq, index) => {
    const oscillator = ctx.createOscillator();
    const gainNode = ctx.createGain();
    
    oscillator.connect(gainNode);
    gainNode.connect(ctx.destination);
    
    oscillator.type = 'square';
    oscillator.frequency.setValueAtTime(freq, ctx.currentTime);
    
    const startTime = ctx.currentTime + (index * 0.1);
    gainNode.gain.setValueAtTime(0, startTime);
    gainNode.gain.linearRampToValueAtTime(0.15, startTime + 0.03);
    gainNode.gain.exponentialRampToValueAtTime(0.01, startTime + 0.3);
    
    oscillator.start(startTime);
    oscillator.stop(startTime + 0.3);
  });
}

/**
 * Play button click feedback
 */
export function playClickSound() {
  const ctx = getAudioContext();
  const oscillator = ctx.createOscillator();
  const gainNode = ctx.createGain();
  
  oscillator.connect(gainNode);
  gainNode.connect(ctx.destination);
  
  oscillator.type = 'sine';
  oscillator.frequency.setValueAtTime(600, ctx.currentTime);
  
  gainNode.gain.setValueAtTime(0.15, ctx.currentTime);
  gainNode.gain.exponentialRampToValueAtTime(0.01, ctx.currentTime + 0.08);
  
  oscillator.start(ctx.currentTime);
  oscillator.stop(ctx.currentTime + 0.08);
}
