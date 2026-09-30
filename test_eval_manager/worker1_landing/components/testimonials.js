// Testimonials Component
export function initTestimonials() {
    const testimonials = document.getElementById('testimonials');
    if (!testimonials) return;
    
    const testimonialsData = [
        {
            quote: "NovaCode Labs transformed our outdated platform into a modern, lightning-fast application. Their attention to detail and technical expertise exceeded our expectations.",
            author: "Sarah Chen",
            role: "CTO at TechFlow Inc.",
            avatar: "SC"
        },
        {
            quote: "Working with NovaCode was a game-changer for our startup. They delivered a polished MVP in record time and continue to be our trusted development partner.",
            author: "Marcus Johnson",
            role: "Founder at HealthSync",
            avatar: "MJ"
        },
        {
            quote: "The team's deep expertise in React and cloud architecture helped us scale from 1,000 to 100,000 users without any issues. Highly recommend!",
            author: "Emily Rodriguez",
            role: "VP Engineering at DataPro",
            avatar: "ER"
        }
    ];
    
    testimonials.innerHTML = `
        <div class="container">
            <div class="section-header">
                <span class="section-label">Testimonials</span>
                <h2 class="section-title">What Our Clients Say</h2>
                <p class="section-subtitle">
                    Don't just take our word for it. Here's what our clients have to say about working with NovaCode Labs.
                </p>
            </div>
            
            <div class="testimonials-grid">
                ${testimonialsData.map((testimonial, index) => `
                    <div class="testimonial-card card" data-index="${index}">
                        <div class="testimonial-quote">
                            <span class="quote-icon">"</span>
                            <p class="quote-text">${testimonial.quote}</p>
                        </div>
                        <div class="testimonial-author">
                            <div class="author-avatar">${testimonial.avatar}</div>
                            <div class="author-info">
                                <span class="author-name">${testimonial.author}</span>
                                <span class="author-role">${testimonial.role}</span>
                            </div>
                        </div>
                    </div>
                `).join('')}
            </div>
        </div>
    `;
    
    addTestimonialsStyles();
}

function addTestimonialsStyles() {
    if (document.getElementById('testimonials-styles')) return;
    
    const style = document.createElement('style');
    style.id = 'testimonials-styles';
    style.textContent = `
        #testimonials {
            background: var(--bg);
        }
        
        #testimonials::before {
            content: '';
            position: absolute;
            top: 0;
            left: 0;
            right: 0;
            height: 1px;
            background: linear-gradient(90deg, transparent, var(--surface), transparent);
        }
        
        .testimonials-grid {
            display: grid;
            grid-template-columns: repeat(3, 1fr);
            gap: var(--space-8);
        }
        
        .testimonial-card {
            display: flex;
            flex-direction: column;
            justify-content: space-between;
        }
        
        .testimonial-quote {
            position: relative;
            margin-bottom: var(--space-6);
        }
        
        .quote-icon {
            font-size: 4rem;
            font-family: Georgia, serif;
            color: var(--accent);
            opacity: 0.3;
            position: absolute;
            top: -20px;
            left: -10px;
            line-height: 1;
        }
        
        .quote-text {
            position: relative;
            font-size: var(--font-size-base);
            line-height: 1.8;
            color: var(--text);
            font-style: italic;
        }
        
        .testimonial-author {
            display: flex;
            align-items: center;
            gap: var(--space-4);
            padding-top: var(--space-6);
            border-top: 1px solid rgba(255, 255, 255, 0.1);
        }
        
        .author-avatar {
            width: 48px;
            height: 48px;
            border-radius: 50%;
            background: linear-gradient(135deg, var(--accent), var(--surface));
            display: flex;
            align-items: center;
            justify-content: center;
            font-weight: 700;
            font-size: var(--font-size-sm);
            color: var(--white);
        }
        
        .author-info {
            display: flex;
            flex-direction: column;
        }
        
        .author-name {
            font-weight: 600;
            color: var(--white);
        }
        
        .author-role {
            font-size: var(--font-size-sm);
            color: var(--text-muted);
        }
        
        @media (max-width: 968px) {
            .testimonials-grid {
                grid-template-columns: 1fr;
                max-width: 500px;
                margin: 0 auto;
            }
        }
    `;
    document.head.appendChild(style);
}
