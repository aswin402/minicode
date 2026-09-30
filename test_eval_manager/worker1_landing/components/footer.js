// Footer Component
export function initFooter() {
    const footer = document.getElementById('footer');
    if (!footer) return;
    
    footer.innerHTML = `
        <div class="container">
            <div class="footer-main">
                <div class="footer-brand">
                    <a href="#" class="footer-logo">
                        <span class="logo-icon">⚡</span>
                        <span class="logo-text">NovaCode Labs</span>
                    </a>
                    <p class="footer-tagline">
                        Building digital experiences that transform businesses.
                    </p>
                    <div class="social-links">
                        <a href="#" class="social-link" aria-label="Twitter">
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/></svg>
                        </a>
                        <a href="#" class="social-link" aria-label="GitHub">
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.477 2 2 6.477 2 12c0 4.42 2.865 8.166 6.839 9.489.5.092.682-.217.682-.482 0-.237-.008-.866-.013-1.7-2.782.604-3.369-1.34-3.369-1.34-.454-1.156-1.11-1.464-1.11-1.464-.908-.62.069-.608.069-.608 1.003.07 1.531 1.03 1.531 1.03.892 1.529 2.341 1.087 2.91.831.092-.646.35-1.086.636-1.336-2.22-.253-4.555-1.11-4.555-4.943 0-1.091.39-1.984 1.029-2.683-.103-.253-.446-1.27.098-2.647 0 0 .84-.269 2.75 1.025A9.578 9.578 0 0112 6.836a9.59 9.59 0 012.504.337c1.909-1.294 2.747-1.025 2.747-1.025.546 1.377.203 2.394.1 2.647.64.699 1.028 1.592 1.028 2.683 0 3.842-2.339 4.687-4.566 4.935.359.309.678.919.678 1.852 0 1.336-.012 2.415-.012 2.743 0 .267.18.578.688.48C19.138 20.163 22 16.418 22 12c0-5.523-4.477-10-10-10z"/></svg>
                        </a>
                        <a href="#" class="social-link" aria-label="LinkedIn">
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.267 2.37 4.267 5.455v6.286zM5.337 7.433a2.062 2.062 0 01-2.063-2.065 2.064 2.064 0 112.063 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z"/></svg>
                        </a>
                    </div>
                </div>
                
                <div class="footer-links">
                    <div class="footer-column">
                        <h4 class="footer-heading">Services</h4>
                        <ul class="footer-list">
                            <li><a href="#features">Web Development</a></li>
                            <li><a href="#features">Mobile Apps</a></li>
                            <li><a href="#features">UI/UX Design</a></li>
                            <li><a href="#features">Cloud Solutions</a></li>
                        </ul>
                    </div>
                    
                    <div class="footer-column">
                        <h4 class="footer-heading">Company</h4>
                        <ul class="footer-list">
                            <li><a href="#">About Us</a></li>
                            <li><a href="#">Our Work</a></li>
                            <li><a href="#">Careers</a></li>
                            <li><a href="#contact">Contact</a></li>
                        </ul>
                    </div>
                    
                    <div class="footer-column">
                        <h4 class="footer-heading">Legal</h4>
                        <ul class="footer-list">
                            <li><a href="#">Privacy Policy</a></li>
                            <li><a href="#">Terms of Service</a></li>
                            <li><a href="#">Cookie Policy</a></li>
                        </ul>
                    </div>
                </div>
            </div>
            
            <div class="footer-bottom">
                <p class="copyright">© 2024 NovaCode Labs. All rights reserved.</p>
                <p class="made-with">Made with ❤️ in San Francisco</p>
            </div>
        </div>
    `;
    
    addFooterStyles();
}

function addFooterStyles() {
    if (document.getElementById('footer-styles')) return;
    
    const style = document.createElement('style');
    style.id = 'footer-styles';
    style.textContent = `
        footer {
            background: var(--surface);
            padding: var(--space-16) 0 var(--space-8);
        }
        
        .footer-main {
            display: grid;
            grid-template-columns: 1.5fr 2fr;
            gap: var(--space-16);
            margin-bottom: var(--space-12);
        }
        
        .footer-logo {
            display: flex;
            align-items: center;
            gap: var(--space-2);
            text-decoration: none;
            color: var(--white);
            font-weight: 700;
            font-size: var(--font-size-xl);
            margin-bottom: var(--space-4);
        }
        
        .footer-tagline {
            color: var(--text-muted);
            margin-bottom: var(--space-6);
            max-width: 300px;
        }
        
        .social-links {
            display: flex;
            gap: var(--space-3);
        }
        
        .social-link {
            width: 40px;
            height: 40px;
            border-radius: var(--radius-lg);
            background: var(--bg);
            display: flex;
            align-items: center;
            justify-content: center;
            color: var(--text);
            transition: all var(--transition-base);
        }
        
        .social-link:hover {
            background: var(--accent);
            color: var(--bg);
            transform: translateY(-2px);
        }
        
        .footer-links {
            display: grid;
            grid-template-columns: repeat(3, 1fr);
            gap: var(--space-8);
        }
        
        .footer-heading {
            font-size: var(--font-size-base);
            font-weight: 700;
            color: var(--white);
            margin-bottom: var(--space-4);
        }
        
        .footer-list {
            list-style: none;
            display: flex;
            flex-direction: column;
            gap: var(--space-3);
        }
        
        .footer-list a {
            color: var(--text-muted);
            text-decoration: none;
            font-size: var(--font-size-sm);
            transition: color var(--transition-fast);
        }
        
        .footer-list a:hover {
            color: var(--accent);
        }
        
        .footer-bottom {
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding-top: var(--space-8);
            border-top: 1px solid var(--bg);
        }
        
        .copyright {
            color: var(--text-muted);
            font-size: var(--font-size-sm);
        }
        
        .made-with {
            color: var(--text-muted);
            font-size: var(--font-size-sm);
        }
        
        @media (max-width: 968px) {
            .footer-main {
                grid-template-columns: 1fr;
                gap: var(--space-10);
            }
            
            .footer-links {
                grid-template-columns: repeat(2, 1fr);
            }
            
            .footer-bottom {
                flex-direction: column;
                gap: var(--space-4);
                text-align: center;
            }
        }
        
        @media (max-width: 640px) {
            .footer-links {
                grid-template-columns: 1fr;
            }
        }
    `;
    document.head.appendChild(style);
}
