// Contact Component
export function initContact() {
    const contact = document.getElementById('contact');
    if (!contact) return;
    
    contact.innerHTML = `
        <div class="container">
            <div class="contact-wrapper">
                <div class="contact-info">
                    <span class="section-label">Get in Touch</span>
                    <h2 class="section-title">Let's Build Something Amazing</h2>
                    <p class="contact-description">
                        Have a project in mind? We'd love to hear about it. Send us a message and we'll get back to you within 24 hours.
                    </p>
                    
                    <div class="contact-methods">
                        <div class="contact-method">
                            <div class="method-icon">📧</div>
                            <div class="method-content">
                                <span class="method-label">Email</span>
                                <span class="method-value">hello@novacodelabs.io</span>
                            </div>
                        </div>
                        <div class="contact-method">
                            <div class="method-icon">📍</div>
                            <div class="method-content">
                                <span class="method-label">Location</span>
                                <span class="method-value">San Francisco, CA</span>
                            </div>
                        </div>
                        <div class="contact-method">
                            <div class="method-icon">⏰</div>
                            <div class="method-content">
                                <span class="method-label">Hours</span>
                                <span class="method-value">Mon-Fri: 9AM - 6PM PST</span>
                            </div>
                        </div>
                    </div>
                </div>
                
                <form class="contact-form" id="contact-form">
                    <div class="form-row">
                        <div class="form-group">
                            <label for="name">Your Name</label>
                            <input type="text" id="name" name="name" placeholder="John Doe" required>
                        </div>
                        <div class="form-group">
                            <label for="email">Email Address</label>
                            <input type="email" id="email" name="email" placeholder="john@company.com" required>
                        </div>
                    </div>
                    <div class="form-group">
                        <label for="service">Service Interested In</label>
                        <select id="service" name="service">
                            <option value="">Select a service...</option>
                            <option value="web">Web Development</option>
                            <option value="mobile">Mobile App Development</option>
                            <option value="design">UI/UX Design</option>
                            <option value="cloud">Cloud Solutions</option>
                            <option value="ai">AI Integration</option>
                            <option value="other">Other</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label for="budget">Project Budget</label>
                        <select id="budget" name="budget">
                            <option value="">Select budget range...</option>
                            <option value="5k-10k">$5,000 - $10,000</option>
                            <option value="10k-25k">$10,000 - $25,000</option>
                            <option value="25k-50k">$25,000 - $50,000</option>
                            <option value="50k+">$50,000+</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label for="message">Project Details</label>
                        <textarea id="message" name="message" rows="5" placeholder="Tell us about your project..." required></textarea>
                    </div>
                    <button type="submit" class="btn btn-primary submit-btn">
                        Send Message
                        <span class="btn-arrow">→</span>
                    </button>
                </form>
            </div>
        </div>
    `;
    
    addContactStyles();
    initFormHandler();
}

function initFormHandler() {
    const form = document.getElementById('contact-form');
    form?.addEventListener('submit', (e) => {
        e.preventDefault();
        
        const submitBtn = form.querySelector('.submit-btn');
        const originalText = submitBtn.innerHTML;
        
        submitBtn.innerHTML = 'Sending...';
        submitBtn.disabled = true;
        
        // Simulate form submission
        setTimeout(() => {
            submitBtn.innerHTML = '✓ Message Sent!';
            submitBtn.style.background = '#10B981';
            
            setTimeout(() => {
                submitBtn.innerHTML = originalText;
                submitBtn.style.background = '';
                submitBtn.disabled = false;
                form.reset();
            }, 2000);
        }, 1500);
    });
}

function addContactStyles() {
    if (document.getElementById('contact-styles')) return;
    
    const style = document.createElement('style');
    style.id = 'contact-styles';
    style.textContent = `
        #contact {
            background: var(--bg);
        }
        
        #contact::before {
            content: '';
            position: absolute;
            top: 0;
            left: 0;
            right: 0;
            height: 1px;
            background: linear-gradient(90deg, transparent, var(--surface), transparent);
        }
        
        .contact-wrapper {
            display: grid;
            grid-template-columns: 1fr 1.2fr;
            gap: var(--space-16);
            align-items: start;
        }
        
        .contact-description {
            color: var(--text-muted);
            margin-bottom: var(--space-8);
            font-size: var(--font-size-lg);
        }
        
        .contact-methods {
            display: flex;
            flex-direction: column;
            gap: var(--space-6);
        }
        
        .contact-method {
            display: flex;
            align-items: center;
            gap: var(--space-4);
        }
        
        .method-icon {
            font-size: var(--font-size-2xl);
        }
        
        .method-content {
            display: flex;
            flex-direction: column;
        }
        
        .method-label {
            font-size: var(--font-size-sm);
            color: var(--text-muted);
        }
        
        .method-value {
            font-weight: 600;
            color: var(--white);
        }
        
        .contact-form {
            background: var(--surface);
            padding: var(--space-8);
            border-radius: var(--radius-xl);
        }
        
        .form-row {
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: var(--space-4);
        }
        
        .form-group {
            margin-bottom: var(--space-5);
        }
        
        .form-group label {
            display: block;
            font-size: var(--font-size-sm);
            font-weight: 600;
            color: var(--text);
            margin-bottom: var(--space-2);
        }
        
        .form-group input,
        .form-group select,
        .form-group textarea {
            width: 100%;
            padding: var(--space-3) var(--space-4);
            background: var(--bg);
            border: 2px solid transparent;
            border-radius: var(--radius-lg);
            color: var(--text);
            font-family: var(--font-family);
            font-size: var(--font-size-base);
            transition: all var(--transition-fast);
        }
        
        .form-group input::placeholder,
        .form-group textarea::placeholder {
            color: var(--text-muted);
        }
        
        .form-group input:focus,
        .form-group select:focus,
        .form-group textarea:focus {
            outline: none;
            border-color: var(--accent);
        }
        
        .form-group select {
            cursor: pointer;
            appearance: none;
            background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23EEEEEE' d='M6 8L1 3h10z'/%3E%3C/svg%3E");
            background-repeat: no-repeat;
            background-position: right var(--space-4) center;
            padding-right: var(--space-10);
        }
        
        .form-group select option {
            background: var(--bg);
            color: var(--text);
        }
        
        .form-group textarea {
            resize: vertical;
            min-height: 120px;
        }
        
        .submit-btn {
            width: 100%;
            padding: var(--space-4);
            font-size: var(--font-size-base);
        }
        
        @media (max-width: 968px) {
            .contact-wrapper {
                grid-template-columns: 1fr;
            }
            
            .form-row {
                grid-template-columns: 1fr;
            }
        }
    `;
    document.head.appendChild(style);
}
