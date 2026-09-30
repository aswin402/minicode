// Pricing Component
export function initPricing() {
    const pricing = document.getElementById('pricing');
    if (!pricing) return;
    
    const plans = [
        {
            name: 'Starter',
            price: '2,500',
            period: '/month',
            description: 'Perfect for small projects and MVPs',
            features: [
                'Up to 3 pages',
                'Responsive design',
                'Basic SEO setup',
                'Email support',
                '2 revisions'
            ],
            highlighted: false,
            cta: 'Get Started'
        },
        {
            name: 'Professional',
            price: '5,000',
            period: '/month',
            description: 'Best for growing businesses',
            features: [
                'Up to 10 pages',
                'Custom functionality',
                'Advanced SEO',
                'Priority support',
                'Unlimited revisions',
                'Analytics integration',
                'CMS integration'
            ],
            highlighted: true,
            cta: 'Start Project',
            badge: 'Most Popular'
        },
        {
            name: 'Enterprise',
            price: 'Custom',
            period: '',
            description: 'For large-scale applications',
            features: [
                'Unlimited pages',
                'Complex integrations',
                'Dedicated team',
                '24/7 support',
                'SLA guarantee',
                'Security audit',
                'Custom AI features'
            ],
            highlighted: false,
            cta: 'Contact Us'
        }
    ];
    
    pricing.innerHTML = `
        <div class="container">
            <div class="section-header">
                <span class="section-label">Pricing</span>
                <h2 class="section-title">Simple, Transparent Pricing</h2>
                <p class="section-subtitle">
                    Choose the plan that fits your needs. All plans include our commitment to quality and client satisfaction.
                </p>
            </div>
            
            <div class="pricing-grid">
                ${plans.map(plan => `
                    <div class="pricing-card card ${plan.highlighted ? 'highlighted' : ''}">
                        ${plan.badge ? `<div class="pricing-badge">${plan.badge}</div>` : ''}
                        <div class="pricing-header">
                            <h3 class="pricing-name">${plan.name}</h3>
                            <p class="pricing-description">${plan.description}</p>
                        </div>
                        <div class="pricing-price">
                            <span class="price-currency">$</span>
                            <span class="price-value">${plan.price}</span>
                            <span class="price-period">${plan.period}</span>
                        </div>
                        <ul class="pricing-features">
                            ${plan.features.map(feature => `
                                <li class="pricing-feature">
                                    <span class="check-icon">✓</span>
                                    <span>${feature}</span>
                                </li>
                            `).join('')}
                        </ul>
                        <a href="#contact" class="btn ${plan.highlighted ? 'btn-primary' : 'btn-secondary'} pricing-cta">
                            ${plan.cta}
                        </a>
                    </div>
                `).join('')}
            </div>
        </div>
    `;
    
    addPricingStyles();
}

function addPricingStyles() {
    if (document.getElementById('pricing-styles')) return;
    
    const style = document.createElement('style');
    style.id = 'pricing-styles';
    style.textContent = `
        #pricing {
            background: var(--bg);
            position: relative;
        }
        
        #pricing::before {
            content: '';
            position: absolute;
            top: 0;
            left: 0;
            right: 0;
            height: 1px;
            background: linear-gradient(90deg, transparent, var(--surface), transparent);
        }
        
        .pricing-grid {
            display: grid;
            grid-template-columns: repeat(3, 1fr);
            gap: var(--space-8);
            align-items: start;
        }
        
        .pricing-card {
            position: relative;
            display: flex;
            flex-direction: column;
        }
        
        .pricing-card.highlighted {
            border: 2px solid var(--accent);
            transform: scale(1.05);
        }
        
        .pricing-badge {
            position: absolute;
            top: -12px;
            left: 50%;
            transform: translateX(-50%);
            background: var(--accent);
            color: var(--bg);
            padding: var(--space-1) var(--space-4);
            border-radius: var(--radius-full);
            font-size: var(--font-size-xs);
            font-weight: 700;
            text-transform: uppercase;
        }
        
        .pricing-header {
            margin-bottom: var(--space-6);
        }
        
        .pricing-name {
            font-size: var(--font-size-2xl);
            font-weight: 700;
            color: var(--white);
            margin-bottom: var(--space-2);
        }
        
        .pricing-description {
            color: var(--text-muted);
            font-size: var(--font-size-sm);
        }
        
        .pricing-price {
            display: flex;
            align-items: baseline;
            margin-bottom: var(--space-6);
            padding-bottom: var(--space-6);
            border-bottom: 1px solid var(--surface);
        }
        
        .price-currency {
            font-size: var(--font-size-xl);
            font-weight: 600;
            color: var(--text-muted);
        }
        
        .price-value {
            font-size: var(--font-size-5xl);
            font-weight: 800;
            color: var(--white);
        }
        
        .price-period {
            color: var(--text-muted);
            margin-left: var(--space-1);
        }
        
        .pricing-features {
            list-style: none;
            margin-bottom: var(--space-8);
            flex-grow: 1;
        }
        
        .pricing-feature {
            display: flex;
            align-items: center;
            gap: var(--space-3);
            padding: var(--space-2) 0;
            color: var(--text);
        }
        
        .check-icon {
            color: var(--accent);
            font-weight: 700;
        }
        
        .pricing-cta {
            width: 100%;
        }
        
        @media (max-width: 968px) {
            .pricing-grid {
                grid-template-columns: 1fr;
                max-width: 400px;
                margin: 0 auto;
            }
            
            .pricing-card.highlighted {
                transform: none;
            }
        }
    `;
    document.head.appendChild(style);
}
