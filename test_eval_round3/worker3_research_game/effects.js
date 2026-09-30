/**
 * QuestDo Visual Effects System
 * Particle systems, animations, and celebration effects
 */

class EffectsEngine {
    constructor() {
        this.canvas = null;
        this.ctx = null;
        this.particles = [];
        this.animationId = null;
        this.isRunning = false;
    }

    /**
     * Initialize the effects engine
     */
    init() {
        this.canvas = document.getElementById('particle-canvas');
        if (!this.canvas) return;
        
        this.ctx = this.canvas.getContext('2d');
        this.resizeCanvas();
        window.addEventListener('resize', () => this.resizeCanvas());
    }

    /**
     * Resize canvas to window size
     */
    resizeCanvas() {
        if (!this.canvas) return;
        this.canvas.width = window.innerWidth;
        this.canvas.height = window.innerHeight;
    }

    /**
     * Start the animation loop
     */
    start() {
        if (this.isRunning) return;
        this.isRunning = true;
        this.animate();
    }

    /**
     * Animation loop
     */
    animate() {
        if (!this.isRunning) return;
        
        this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
        
        // Update and draw particles
        this.particles = this.particles.filter(p => {
            this.updateParticle(p);
            this.drawParticle(p);
            return p.life > 0;
        });
        
        this.animationId = requestAnimationFrame(() => this.animate());
    }

    /**
     * Stop animation when no particles
     */
    stop() {
        if (this.particles.length === 0) {
            this.isRunning = false;
            if (this.animationId) {
                cancelAnimationFrame(this.animationId);
            }
        }
    }

    /**
     * Update particle physics
     */
    updateParticle(p) {
        p.x += p.vx;
        p.y += p.vy;
        p.vy += p.gravity; // Gravity
        p.life -= p.decay;
        p.rotation += p.rotationSpeed;
        
        // Apply friction
        p.vx *= 0.99;
        p.vy *= 0.99;
    }

    /**
     * Draw a single particle
     */
    drawParticle(p) {
        if (!this.ctx) return;
        
        const alpha = Math.min(p.life, 1);
        this.ctx.save();
        this.ctx.translate(p.x, p.y);
        this.ctx.rotate(p.rotation);
        this.ctx.globalAlpha = alpha;
        
        switch (p.shape) {
            case 'circle':
                this.ctx.beginPath();
                this.ctx.arc(0, 0, p.size, 0, Math.PI * 2);
                this.ctx.fillStyle = p.color;
                this.ctx.fill();
                break;
                
            case 'square':
                this.ctx.fillStyle = p.color;
                this.ctx.fillRect(-p.size / 2, -p.size / 2, p.size, p.size);
                break;
                
            case 'star':
                this.drawStar(p);
                break;
                
            default:
                this.ctx.beginPath();
                this.ctx.arc(0, 0, p.size, 0, Math.PI * 2);
                this.ctx.fillStyle = p.color;
                this.ctx.fill();
        }
        
        this.ctx.restore();
    }

    /**
     * Draw star shape
     */
    drawStar(p) {
        const spikes = 5;
        const outerRadius = p.size;
        const innerRadius = p.size / 2;
        
        this.ctx.beginPath();
        for (let i = 0; i < spikes * 2; i++) {
            const radius = i % 2 === 0 ? outerRadius : innerRadius;
            const angle = (i * Math.PI) / spikes - Math.PI / 2;
            const x = Math.cos(angle) * radius;
            const y = Math.sin(angle) * radius;
            
            if (i === 0) {
                this.ctx.moveTo(x, y);
            } else {
                this.ctx.lineTo(x, y);
            }
        }
        this.ctx.closePath();
        this.ctx.fillStyle = p.color;
        this.ctx.fill();
    }

    /**
     * Trigger a celebration effect
     */
    triggerCelebration(type, originX, originY) {
        if (!this.canvas) return;
        
        this.start();
        
        switch (type) {
            case 'taskComplete':
                this.createTaskCompleteEffect(originX, originY);
                break;
            case 'levelUp':
                this.createLevelUpEffect();
                break;
            case 'streak':
                this.createStreakEffect();
                break;
            case 'achievement':
                this.createConfettiEffect();
                break;
            default:
                this.createGenericEffect(originX, originY);
        }
    }

    /**
     * Task complete particle burst
     */
    createTaskCompleteEffect(x, y) {
        const colors = ['#00d26a', '#2ecc71', '#27ae60', '#f4d03f', '#3498db'];
        const count = 20;
        
        for (let i = 0; i < count; i++) {
            const angle = (Math.PI * 2 * i) / count + Math.random() * 0.3;
            const speed = 3 + Math.random() * 4;
            
            this.particles.push({
                x: x || this.canvas.width / 2,
                y: y || this.canvas.height / 2,
                vx: Math.cos(angle) * speed,
                vy: Math.sin(angle) * speed - 2,
                size: 4 + Math.random() * 6,
                color: colors[Math.floor(Math.random() * colors.length)],
                life: 1,
                decay: 0.02 + Math.random() * 0.02,
                gravity: 0.1,
                shape: 'circle',
                rotation: 0,
                rotationSpeed: 0
            });
        }
    }

    /**
     * Level up golden explosion
     */
    createLevelUpEffect() {
        const colors = ['#f4d03f', '#d4a539', '#f39c12', '#e67e22', '#ffffff'];
        const centerX = this.canvas.width / 2;
        const centerY = this.canvas.height / 2;
        const count = 50;
        
        for (let i = 0; i < count; i++) {
            const angle = Math.random() * Math.PI * 2;
            const speed = 2 + Math.random() * 6;
            
            this.particles.push({
                x: centerX,
                y: centerY,
                vx: Math.cos(angle) * speed,
                vy: Math.sin(angle) * speed - 3,
                size: 5 + Math.random() * 10,
                color: colors[Math.floor(Math.random() * colors.length)],
                life: 1,
                decay: 0.01 + Math.random() * 0.015,
                gravity: 0.08,
                shape: Math.random() > 0.5 ? 'star' : 'circle',
                rotation: Math.random() * Math.PI * 2,
                rotationSpeed: (Math.random() - 0.5) * 0.2
            });
        }
    }

    /**
     * Streak fire effect
     */
    createStreakEffect() {
        const colors = ['#e67e22', '#f39c12', '#e74c3c', '#f4d03f'];
        const centerX = this.canvas.width / 2;
        const bottomY = this.canvas.height - 50;
        const count = 30;
        
        for (let i = 0; i < count; i++) {
            const spreadX = (Math.random() - 0.5) * 200;
            
            this.particles.push({
                x: centerX + spreadX,
                y: bottomY,
                vx: (Math.random() - 0.5) * 2,
                vy: -3 - Math.random() * 4,
                size: 3 + Math.random() * 5,
                color: colors[Math.floor(Math.random() * colors.length)],
                life: 1,
                decay: 0.03 + Math.random() * 0.02,
                gravity: -0.05, // Float upward
                shape: 'circle',
                rotation: 0,
                rotationSpeed: 0
            });
        }
    }

    /**
     * Confetti explosion for achievements
     */
    createConfettiEffect() {
        const colors = ['#f4d03f', '#e74c3c', '#3498db', '#2ecc71', '#9b59b6', '#1abc9c', '#e67e22'];
        const count = 80;
        const centerX = this.canvas.width / 2;
        const centerY = this.canvas.height / 2;
        
        for (let i = 0; i < count; i++) {
            const angle = Math.random() * Math.PI * 2;
            const speed = 4 + Math.random() * 8;
            
            this.particles.push({
                x: centerX,
                y: centerY,
                vx: Math.cos(angle) * speed,
                vy: Math.sin(angle) * speed - 5,
                size: 6 + Math.random() * 8,
                color: colors[Math.floor(Math.random() * colors.length)],
                life: 1,
                decay: 0.008 + Math.random() * 0.008,
                gravity: 0.12,
                shape: Math.random() > 0.5 ? 'square' : 'circle',
                rotation: Math.random() * Math.PI * 2,
                rotationSpeed: (Math.random() - 0.5) * 0.3
            });
        }
    }

    /**
     * Generic particle effect
     */
    createGenericEffect(x, y) {
        const colors = ['#4facfe', '#00f2fe', '#f4d03f', '#00d26a'];
        const count = 15;
        
        for (let i = 0; i < count; i++) {
            const angle = (Math.PI * 2 * i) / count;
            
            this.particles.push({
                x: x || this.canvas.width / 2,
                y: y || this.canvas.height / 2,
                vx: Math.cos(angle) * 3,
                vy: Math.sin(angle) * 3,
                size: 4 + Math.random() * 4,
                color: colors[Math.floor(Math.random() * colors.length)],
                life: 1,
                decay: 0.025,
                gravity: 0.1,
                shape: 'circle',
                rotation: 0,
                rotationSpeed: 0
            });
        }
    }

    /**
     * Screen shake effect
     */
    shakeScreen(duration = 300) {
        const app = document.getElementById('app');
        if (!app) return;
        
        app.classList.add('shake');
        setTimeout(() => app.classList.remove('shake'), duration);
    }

    /**
     * Glow pulse effect on element
     */
    pulseGlow(element) {
        if (!element) return;
        element.classList.add('glow-pulse');
        setTimeout(() => element.classList.remove('glow-pulse'), 500);
    }

    /**
     * Create floating XP text
     */
    createXPText(xpAmount, originElement) {
        const popup = document.createElement('div');
        popup.className = 'xp-popup';
        popup.textContent = `+${xpAmount} XP`;
        
        const rect = originElement.getBoundingClientRect();
        popup.style.left = `${rect.left + rect.width / 2}px`;
        popup.style.top = `${rect.top}px`;
        
        document.body.appendChild(popup);
        
        setTimeout(() => popup.remove(), 1500);
    }
}

// Export singleton instance
const effects = new EffectsEngine();
