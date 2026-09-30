// Navbar Component
export function initNavbar() {
    const navbar = document.getElementById('navbar');
    if (!navbar) return;
    
    navbar.innerHTML = `
        <div class="navbar-container">
            <a href="#" class="navbar-logo">
                <span class="logo-icon">⚡</span>
                <span class="logo-text">NovaCode</span>
            </a>
            
            <button class="navbar-toggle" id="navbar-toggle" aria-label="Toggle navigation">
                <span></span>
                <span></span>
                <span></span>
            </button>
            
            <div class="navbar-menu" id="navbar-menu">
                <ul class="navbar-links">
                    <li><a href="#features">Features</a></li>
                    <li><a href="#pricing">Pricing</a></li>
                    <li><a href="#testimonials">Testimonials</a></li>
                    <li><a href="#contact">Contact</a></li>
                </ul>
                <div class="navbar-actions">
                    <a href="#contact" class="btn btn-secondary">Get Started</a>
                </div>
            </div>
        </div>
    `;
    
    // Add navbar styles
    addNavbarStyles();
    
    // Toggle mobile menu
    const toggle = document.getElementById('navbar-toggle');
    const menu = document.getElementById('navbar-menu');
    
    toggle?.addEventListener('click', () => {
        menu.classList.toggle('active');
        toggle.classList.toggle('active');
    });
    
    // Add scroll effect
    window.addEventListener('scroll', () => {
        if (window.scrollY > 50) {
            navbar.classList.add('scrolled');
        } else {
            navbar.classList.remove('scrolled');
        }
    });
}

function addNavbarStyles() {
    if (document.getElementById('navbar-styles')) return;
    
    const style = document.createElement('style');
    style.id = 'navbar-styles';
    style.textContent = `
        #navbar {
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            z-index: var(--z-fixed);
            background: transparent;
            transition: all var(--transition-base);
        }
        
        #navbar.scrolled {
            background: rgba(34, 40, 49, 0.95);
            backdrop-filter: blur(10px);
            box-shadow: var(--shadow-lg);
        }
        
        .navbar-container {
            max-width: var(--container-max);
            margin: 0 auto;
            padding: var(--space-4) var(--space-6);
            display: flex;
            align-items: center;
            justify-content: space-between;
        }
        
        .navbar-logo {
            display: flex;
            align-items: center;
            gap: var(--space-2);
            text-decoration: none;
            color: var(--white);
            font-weight: 700;
            font-size: var(--font-size-xl);
        }
        
        .logo-icon {
            font-size: var(--font-size-2xl);
        }
        
        .navbar-toggle {
            display: none;
            flex-direction: column;
            gap: 5px;
            background: none;
            border: none;
            cursor: pointer;
            padding: var(--space-2);
        }
        
        .navbar-toggle span {
            width: 24px;
            height: 2px;
            background: var(--text);
            transition: all var(--transition-base);
        }
        
        .navbar-toggle.active span:nth-child(1) {
            transform: rotate(45deg) translate(5px, 5px);
        }
        
        .navbar-toggle.active span:nth-child(2) {
            opacity: 0;
        }
        
        .navbar-toggle.active span:nth-child(3) {
            transform: rotate(-45deg) translate(5px, -5px);
        }
        
        .navbar-menu {
            display: flex;
            align-items: center;
            gap: var(--space-8);
        }
        
        .navbar-links {
            display: flex;
            list-style: none;
            gap: var(--space-6);
        }
        
        .navbar-links a {
            color: var(--text);
            text-decoration: none;
            font-weight: 500;
            transition: color var(--transition-fast);
        }
        
        .navbar-links a:hover {
            color: var(--accent);
        }
        
        .navbar-actions {
            display: flex;
            align-items: center;
            gap: var(--space-4);
        }
        
        @media (max-width: 768px) {
            .navbar-toggle {
                display: flex;
            }
            
            .navbar-menu {
                position: absolute;
                top: 100%;
                left: 0;
                right: 0;
                flex-direction: column;
                background: var(--bg);
                padding: var(--space-6);
                gap: var(--space-4);
                display: none;
                box-shadow: var(--shadow-lg);
            }
            
            .navbar-menu.active {
                display: flex;
            }
            
            .navbar-links {
                flex-direction: column;
                width: 100%;
                text-align: center;
            }
            
            .navbar-actions {
                width: 100%;
                justify-content: center;
            }
        }
    `;
    document.head.appendChild(style);
}
