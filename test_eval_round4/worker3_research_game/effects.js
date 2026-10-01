/**
 * QuestDo — Visual Effects Engine
 * Canvas-based confetti particle system — zero external dependencies
 */

const EffectsEngine = (() => {
  // ── Confetti ──────────────────────────────────────────────────────────────

  let confettiCanvas = null;
  let confettiCtx = null;
  let particles = [];

  const PALETTE = [
    '#f7b731', '#eb3b5a', '#3867d6', '#20bf6b',
    '#fd79a8', '#a29bfe', '#fdcb6e', '#00cec9'
  ];

  const initConfetti = () => {
    if (confettiCanvas) return;
    confettiCanvas = document.createElement('canvas');
    confettiCtx = confettiCanvas.getContext('2d');
    Object.assign(confettiCanvas.style, {
      position: 'fixed', top: '0', left: '0', width: '100%', height: '100%',
      pointerEvents: 'none', zIndex: '9999', opacity: '1'
    });
    document.body.appendChild(confettiCanvas);
    resizeConfetti();
    window.addEventListener('resize', resizeConfetti);
  };

  const resizeConfetti = () => {
    if (!confettiCanvas) return;
    confettiCanvas.width = window.innerWidth;
    confettiCanvas.height = window.innerHeight;
  };

  const createParticle = (x, y) => ({
    x, y,
    vx: (Math.random() - 0.5) * 12,
    vy: Math.random() * -14 - 4,
    color: PALETTE[Math.floor(Math.random() * PALETTE.length)],
    size: Math.random() * 8 + 4,
    rotation: Math.random() * 360,
    rotationSpeed: (Math.random() - 0.5) * 15,
    shape: Math.random() > 0.5 ? 'rect' : 'circle',
    life: 1.0,
    decay: 0.008 + Math.random() * 0.006,
    gravity: 0.45
  });

  const burst = (x, y, count = 60) => {
    initConfetti();
    for (let i = 0; i < count; i++) {
      particles.push(createParticle(x, y));
    }
    if (!confettiCanvas.dataset.animating) {
      confettiCanvas.dataset.animating = '1';
      animateConfetti();
    }
  };

  const burstScreenCenter = (count = 60) => {
    burst(window.innerWidth / 2, window.innerHeight / 2, count);
  };

  const animateConfetti = () => {
    if (!confettiCtx) return;
    confettiCtx.clearRect(0, 0, confettiCanvas.width, confettiCanvas.height);

    particles = particles.filter(p => {
      p.x += p.vx;
      p.y += p.vy;
      p.vy += p.gravity;
      p.rotation += p.rotationSpeed;
      p.life -= p.decay;
      p.vx *= 0.99;

      if (p.life <= 0) return false;

      confettiCtx.save();
      confettiCtx.globalAlpha = Math.max(0, p.life);
      confettiCtx.translate(p.x, p.y);
      confettiCtx.rotate((p.rotation * Math.PI) / 180);
      confettiCtx.fillStyle = p.color;

      if (p.shape === 'rect') {
        confettiCtx.fillRect(-p.size / 2, -p.size / 4, p.size, p.size / 2);
      } else {
        confettiCtx.beginPath();
        confettiCtx.arc(0, 0, p.size / 2, 0, Math.PI * 2);
        confettiCtx.fill();
      }
      confettiCtx.restore();
      return true;
    });

    if (particles.length > 0) {
      requestAnimationFrame(animateConfetti);
    } else {
      confettiCanvas.dataset.animating = '';
    }
  };

  // ── XP Float Text ─────────────────────────────────────────────────────────

  const showXPFloat = (amount, x, y) => {
    const el = document.createElement('div');
    el.className = 'xp-float';
    el.textContent = `+${amount} XP`;
    el.style.cssText = `position:fixed;left:${x}px;top:${y}px;pointer-events:none;z-index:9998;
      font-size:1.4rem;font-weight:bold;color:#f7b731;text-shadow:0 0 8px rgba(247,183,49,.8);
      animation:xpFloatAnim 1s ease-out forwards;transform:translateX(-50%);`;
    document.body.appendChild(el);
    setTimeout(() => el.remove(), 1000);
  };

  // ── Level Up Overlay ───────────────────────────────────────────────────────

  const showLevelUp = (level, perk) => {
    const overlay = document.createElement('div');
    overlay.id = 'levelup-overlay';
    overlay.style.cssText = `position:fixed;inset:0;background:rgba(0,0,0,.88);display:flex;
      flex-direction:column;align-items:center;justify-content:center;z-index:9997;
      animation:fadeInOverlay .3s ease-out;`;

    const title = document.createElement('div');
    title.style.cssText = `font-size:3rem;font-weight:900;color:#f7b731;text-shadow:0 0 30px rgba(247,183,49,.9);
      margin-bottom:.5rem;animation:levelUpTitleAnim 1.2s ease-out forwards;`;
    title.textContent = `⬆ LEVEL ${level}!`;

    const subtitle = document.createElement('div');
    subtitle.style.cssText = `font-size:1.3rem;color:#a29bfe;margin-bottom:1rem;`;
    subtitle.textContent = levelTitle;

    const perkBox = document.createElement('div');
    perkBox.style.cssText = `background:rgba(162,155,254,.15);border:2px solid #a29bfe;
      border-radius:12px;padding:1rem 2rem;margin:1rem 0;max-width:420px;text-align:center;`;
    const perkTitle = perk?.name || 'New Perk';
    const perkDesc = perk?.desc || 'Keep leveling to unlock more!';
    const levelTitle = (() => {
      const brackets = [
        { min: 1, max: 4, title: 'Apprentice' },
        { min: 5, max: 8, title: 'Adept' },
        { min: 9, max: 12, title: 'Master' },
        { min: 13, max: 999, title: 'Legend' }
      ];
      return brackets.find(b => level >= b.min && level <= b.max)?.title || 'Legend';
    })();
    perkBox.innerHTML = `<div style="color:#fd79a8;font-size:.85rem;letter-spacing:2px;margin-bottom:.5rem;">✨ NEW PERK UNLOCKED ✨</div>
      <div style="color:#fff;font-size:1.1rem;font-weight:700;">${perkTitle}</div>
      <div style="color:#ccc;font-size:.9rem;margin-top:.4rem;">${perkDesc}</div>`;

    const btn = document.createElement('button');
    btn.textContent = '⚔️ Continue Quest!';
    btn.style.cssText = `margin-top:1.5rem;padding:.8rem 2.5rem;font-size:1.1rem;font-weight:700;
      background:linear-gradient(135deg,#f7b731,#eb3b5a);color:#fff;border:none;border-radius:50px;
      cursor:pointer;box-shadow:0 4px 20px rgba(247,183,49,.4);transition:transform .15s;`;
    btn.addEventListener('mouseenter', () => btn.style.transform = 'scale(1.06)');
    btn.addEventListener('mouseleave', () => btn.style.transform = 'scale(1)');
    btn.addEventListener('click', () => {
      overlay.style.animation = 'fadeOutOverlay .3s ease-out forwards';
      setTimeout(() => overlay.remove(), 300);
    });

    overlay.append(title, subtitle, perkBox, btn);
    document.body.appendChild(overlay);
    burstScreenCenter(90);
  };

  return { burst, burstScreenCenter, showXPFloat, showLevelUp, initConfetti };
})();

// Inject animations into <head>
const injectEffectStyles = () => {
  const css = `
    @keyframes xpFloatAnim {
      0%   { opacity:1; transform:translateX(-50%) translateY(0) scale(1); }
      60%  { opacity:1; transform:translateX(-50%) translateY(-50px) scale(1.2); }
      100% { opacity:0; transform:translateX(-50%) translateY(-90px) scale(.9); }
    }
    @keyframes levelUpTitleAnim {
      0%   { transform:scale(.3) rotate(-10deg); opacity:0; }
      50%  { transform:scale(1.15) rotate(2deg); opacity:1; }
      70%  { transform:scale(.95) rotate(-1deg); }
      100% { transform:scale(1) rotate(0deg); opacity:1; }
    }
    @keyframes fadeInOverlay {
      from { opacity:0; } to { opacity:1; }
    }
    @keyframes fadeOutOverlay {
      from { opacity:1; } to { opacity:0; }
    }
    @keyframes streakFire {
      0%,100% { text-shadow:0 0 6px #eb3b5a,0 0 14px #eb3b5a,0 0 22px #f7b731; }
      50%      { text-shadow:0 0 12px #eb3b5a,0 0 24px #f7b731,0 0 36px #fd79a8; }
    }
    @keyframes taskCompletePulse {
      0%   { transform:scale(1); }
      30%  { transform:scale(1.04); }
      100% { transform:scale(1); }
    }
    @keyframes xpBarGlow {
      0%,100% { box-shadow:0 0 8px rgba(247,183,49,.4); }
      50%      { box-shadow:0 0 20px rgba(247,183,49,.9); }
    }
    @keyframes levelBadgePulse {
      0%,100% { transform:scale(1); box-shadow:0 0 8px rgba(255,255,255,.3); }
      50%      { transform:scale(1.12); box-shadow:0 0 22px rgba(247,183,49,.8); }
    }
    @keyframes shimmer {
      0%   { background-position:-200% center; }
      100% { background-position:200% center; }
    }
    @keyframes floatUp {
      0%   { transform:translateY(0); opacity:.9; }
      100% { transform:translateY(-8px); opacity:.5; }
    }
    .task-card.completing { animation:taskCompletePulse .3s ease-out; }
    .streak-fire { animation:streakFire 1.2s ease-in-out infinite; }
    .xp-bar-fill.animating { animation:xpBarGlow .6s ease-in-out 2; }
    .level-badge.leveling { animation:levelBadgePulse .8s ease-in-out; }
  `;
  const style = document.createElement('style');
  style.textContent = css;
  document.head.appendChild(style);
};
