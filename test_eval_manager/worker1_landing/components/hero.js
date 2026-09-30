// Hero Component
export function initHero() {
    const hero = document.getElementById('hero');
    if (!hero) return;
    
    hero.innerHTML = `
        <div class="hero-container container">
            <div class="hero-content">
                <div class="hero-badge">
                    <span class="badge-icon">🚀</span>
                    <span>Now building with AI-powered tools</span>
                </div>
                
                <h1 class="hero-title">
                    We Build <span class="text-accent">Digital Products</span> That Scale
                </h1>
                
                <p class="hero-subtitle">
                    NovaCode Labs is a creative software agency crafting exceptional web and mobile experiences. From MVPs to enterprise solutions, we turn your vision into reality.
                </p>
                
                <div class="hero-actions">
                    <a href="#contact" class="btn btn-primary">
                        Start Your Project
                        <span class="btn-arrow">→</span>
                    </a>
                    <a href="#features" class="btn btn-secondary">
                        Explore Services
                    </a>
                </div>
                
                <div class="hero-stats">
                    <div class="stat">
                        <span class="stat-value">150+</span>
                        <span class="stat-label">Projects Delivered</span>
                    </div>
                    <div class="stat">
                        <span class="stat-value">50+</span>
                        <span class="stat-label">Happy Clients</span>
                    </div>
                    <div class="stat">
                        <span class="stat-value">8+</span>
                        <span class="stat-label">Years Experience</span>
                    </div>
                </div>
            </div>
            
            <div class="hero-visual">
                <div class="hero-glow"></div>
                <div class="hero-card-stack">
                    <div class="hero-card card-1">
                        <div class="card-icon">🎨</div>
                        <div class="card-content">
                            <span class="card-title">UI/UX Design</span>
                            <span class="card-desc">Pixel-perfect interfaces</span>
                        </div>
                    </div>
                    <div class="hero-card card-2">
                        <div class="card-icon">⚡</div>
                        <div class="card-content">
                            <span class="card-title">Lightning Fast</span>
                            <span class="card-desc">Performance optimized</span>
                        </div>
                    </div>
                    <div class="hero-card card-3">
                        <div class="card-icon">🔒</div>
                        <div class="card-content">
                            <span class="card-title">Secure Code</span>
                            <span class="card-desc">Enterprise grade</span>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    `;
    
    addHeroStyles();
}

function addHeroStyles() {
    if (document.getElementById('hero-styles')) return;
    
    const style = document.createElement('style');
    style.id = 'hero-styles';
    style.textContent = `
        #hero {
            min-height: 100vh;
            display: flex;
            align-items: center;
            padding-top: 80px;
            position: relative;
            overflow: hidden;
        }
        
        .hero-container {
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: var(--space-16);
            align-items: center;
        }
        
        .hero-content {
            animation: fadeInUp 0.8s ease forwards;
        }
        
        .hero-badge {
            display: inline-flex;
            align-items: center;
            gap: var(--space-2);
            background: var(--surface);
            padding: var(--space-2) var(--space-4);
            border-radius: var(--radius-full);
            font-size: var(--font-size-sm);
            margin-bottom: var(--space-6);
        }
        
        .badge-icon {
            font-size: var(--font-size-lg);
        }
        
        .hero-title {
            font-size: var(--font-size-5xl);
            font-weight: 800;
            line-height: 1.1;
            color: var(--white);
            margin-bottom: var(--space-6);
        }
        
        .hero-subtitle {
            font-size: var(--font-size-lg);
            color: var(--text-muted);
            margin-bottom: var(--space-8);
            max-width: 500px;
        }
        
        .hero-actions {
            display: flex;
            gap: var(--space-4);
            margin-bottom: var(--space-12);
        }
        
        .btn-arrow {
            transition: transform var(--transition-fast);
        }
        
        .btn:hover .btn-arrow {
            transform: translateX(4px);
        }
        
        .hero-stats {
            display: flex;
            gap: var(--space-10);
        }
        
        .stat {
            display: flex;
            flex-direction: column;
        }
        
        .stat-value {
            font-size: var(--font-size-3xl);
            font-weight: 700;
            color: var(--white);
        }
        
        .stat-label {
            font-size: var(--font-size-sm);
            color: var(--text-muted);
        }
        
        .hero-visual {
            position: relative;
            height: 400px;
        }
        
        .hero-glow {
            position: absolute;
            top: 50%;
            left: 50%;
            transform: translate(-50%, -50%);
            width: 300px;
            height: 300px;
            background: radial-gradient(circle, var(--accent-glow) 0%, transparent 70%);
            border-radius: 50%;
            animation: pulse 4s ease-in-out infinite;
        }
        
        .hero-card-stack {
            position: relative;
            height: 100%;
        }
        
        .hero-card {
            position: absolute;
            background: var(--surface);
            border-radius: var(--radius-xl);
            padding: var(--space-5);
            display: flex;
            align-items: center;
            gap: var(--space-4);
            box-shadow: var(--shadow-xl);
            animation: float 6s ease-in-out infinite;
        }
        
        .hero-card .card-icon {
            font-size: var(--font-size-2xl);
        }
        
        .hero-card .card-content {
            display: flex;
            flex-direction: column;
        }
        
        .hero-card .card-title {
            font-weight: 600;
            color: var(--white);
        }
        
        .hero-card .card-desc {
            font-size: var(--font-size-sm);
            color: var(--text-muted);
        }
        
        .card-1 {
            top: 10%;
            left: 20%;
            animation-delay: 0s;
        }
        
        .card-2 {
            top: 45%;
            right: 10%;
            animation-delay: 2s;
        }
        
        .card-3 {
            bottom: 15%;
            left: 30%;
            animation-delay: 4s;
        }
        
        @keyframes float {
            0%, 100% { transform: translateY(0px); }
            50% { transform: translateY(-15px); }
        }
        
        @media (max-width: 968px) {
            .hero-container {
                grid-template-columns: 1fr;
                text-align: center;
            }
            
            .hero-subtitle {
                margin-left: auto;
                margin-right: auto;
            }
            
            .hero-actions {
                justify-content: center;
            }
            
            .hero-stats {
                justify-content: center;
            }
            
            .hero-visual {
                display: none;
            }
        }
    `;
    document.head.appendChild(style);
}
