// Features Component
export function initFeatures() {
    const features = document.getElementById('features');
    if (!features) return;
    
    const featuresData = [
        {
            icon: '🎨',
            title: 'UI/UX Design',
            description: 'Beautiful, intuitive interfaces that users love. We create designs that are both aesthetically pleasing and highly functional.'
        },
        {
            icon: '💻',
            title: 'Web Development',
            description: 'Modern web applications built with cutting-edge technologies. React, Vue, Next.js, and more.'
        },
        {
            icon: '📱',
            title: 'Mobile Apps',
            description: 'Native and cross-platform mobile experiences. iOS, Android, and React Native expertise.'
        },
        {
            icon: '☁️',
            title: 'Cloud Solutions',
            description: 'Scalable infrastructure on AWS, GCP, and Azure. We handle deployment, CI/CD, and DevOps.'
        },
        {
            icon: '🤖',
            title: 'AI Integration',
            description: 'Smart features powered by machine learning. Chatbots, recommendations, and automation.'
        },
        {
            icon: '🔒',
            title: 'Security First',
            description: 'Enterprise-grade security practices. OWASP compliance, penetration testing, and monitoring.'
        }
    ];
    
    features.innerHTML = `
        <div class="container">
            <div class="section-header">
                <span class="section-label">What We Do</span>
                <h2 class="section-title">Services Built for Impact</h2>
                <p class="section-subtitle">
                    From concept to launch, we deliver comprehensive digital solutions that drive real business results.
                </p>
            </div>
            
            <div class="features-grid">
                ${featuresData.map((feature, index) => `
                    <div class="feature-card card" data-index="${index}">
                        <div class="feature-icon">${feature.icon}</div>
                        <h3 class="feature-title">${feature.title}</h3>
                        <p class="feature-description">${feature.description}</p>
                        <div class="feature-link">
                            <span>Learn more</span>
                            <span class="arrow">→</span>
                        </div>
                    </div>
                `).join('')}
            </div>
        </div>
    `;
    
    addFeaturesStyles();
}

function addFeaturesStyles() {
    if (document.getElementById('features-styles')) return;
    
    const style = document.createElement('style');
    style.id = 'features-styles';
    style.textContent = `
        #features {
            background: var(--bg);
            position: relative;
        }
        
        .features-grid {
            display: grid;
            grid-template-columns: repeat(3, 1fr);
            gap: var(--space-8);
        }
        
        .feature-card {
            position: relative;
            overflow: hidden;
            cursor: pointer;
        }
        
        .feature-card::before {
            content: '';
            position: absolute;
            top: 0;
            left: 0;
            right: 0;
            height: 3px;
            background: linear-gradient(90deg, var(--accent), transparent);
            transform: scaleX(0);
            transform-origin: left;
            transition: transform var(--transition-base);
        }
        
        .feature-card:hover::before {
            transform: scaleX(1);
        }
        
        .feature-icon {
            font-size: 3rem;
            margin-bottom: var(--space-4);
        }
        
        .feature-title {
            font-size: var(--font-size-xl);
            font-weight: 700;
            color: var(--white);
            margin-bottom: var(--space-3);
        }
        
        .feature-description {
            color: var(--text-muted);
            margin-bottom: var(--space-4);
            line-height: 1.7;
        }
        
        .feature-link {
            display: flex;
            align-items: center;
            gap: var(--space-2);
            color: var(--accent);
            font-weight: 600;
            font-size: var(--font-size-sm);
            opacity: 0;
            transform: translateY(10px);
            transition: all var(--transition-base);
        }
        
        .feature-card:hover .feature-link {
            opacity: 1;
            transform: translateY(0);
        }
        
        .feature-link .arrow {
            transition: transform var(--transition-fast);
        }
        
        .feature-card:hover .arrow {
            transform: translateX(4px);
        }
        
        @media (max-width: 968px) {
            .features-grid {
                grid-template-columns: repeat(2, 1fr);
            }
        }
        
        @media (max-width: 640px) {
            .features-grid {
                grid-template-columns: 1fr;
            }
        }
    `;
    document.head.appendChild(style);
}
