/**
 * ApexCloud Landing Page - JavaScript
 * Handles: Navigation, Pricing Toggle, Scroll Animations, Mobile Menu
 */

(function() {
    'use strict';

    // ========================================
    // DOM Elements
    // ========================================
    const navbar = document.getElementById('navbar');
    const mobileMenuBtn = document.getElementById('mobileMenuBtn');
    const mobileMenu = document.getElementById('mobileMenu');
    const pricingToggle = document.getElementById('pricingToggle');
    const priceAmounts = document.querySelectorAll('.price-amount');
    const navLinks = document.querySelectorAll('.nav-link, .mobile-nav-link');
    const animatedElements = document.querySelectorAll('[data-animate]');

    // ========================================
    // Navbar Scroll Behavior
    // ========================================
    function handleNavbarScroll() {
        if (window.scrollY > 50) {
            navbar.classList.add('scrolled');
        } else {
            navbar.classList.remove('scrolled');
        }
    }

    // ========================================
    // Mobile Menu Toggle
    // ========================================
    function toggleMobileMenu() {
        mobileMenu.classList.toggle('active');
        mobileMenuBtn.classList.toggle('active');
        document.body.style.overflow = mobileMenu.classList.contains('active') ? 'hidden' : '';
    }

    function closeMobileMenu() {
        mobileMenu.classList.remove('active');
        mobileMenuBtn.classList.remove('active');
        document.body.style.overflow = '';
    }

    // ========================================
    // Pricing Toggle (Monthly/Yearly)
    // ========================================
    let isYearly = false;

    function togglePricing() {
        isYearly = !isYearly;
        pricingToggle.classList.toggle('yearly', isYearly);

        // Update price amounts with animation
        priceAmounts.forEach(amount => {
            const monthly = amount.dataset.monthly;
            const yearly = amount.dataset.yearly;
            const newPrice = isYearly ? yearly : monthly;
            
            // Animate price change
            amount.style.opacity = '0';
            amount.style.transform = 'translateY(-10px)';
            
            setTimeout(() => {
                amount.textContent = newPrice;
                amount.style.opacity = '1';
                amount.style.transform = 'translateY(0)';
            }, 150);
        });

        // Update toggle labels
        const labels = document.querySelectorAll('.toggle-label');
        labels.forEach((label, index) => {
            if ((isYearly && index === 1) || (!isYearly && index === 0)) {
                label.dataset.active = '';
            } else {
                delete label.dataset.active;
            }
        });
    }

    // ========================================
    // Scroll Animations (Intersection Observer)
    // ========================================
    function setupScrollAnimations() {
        const observerOptions = {
            root: null,
            rootMargin: '0px 0px -50px 0px',
            threshold: 0.1
        };

        const observer = new IntersectionObserver((entries) => {
            entries.forEach((entry, index) => {
                if (entry.isIntersecting) {
                    // Add staggered delay for grid items
                    const parent = entry.target.parentElement;
                    const siblings = Array.from(parent.children).filter(child => 
                        child.hasAttribute('data-animate')
                    );
                    const siblingIndex = siblings.indexOf(entry.target);
                    
                    setTimeout(() => {
                        entry.target.classList.add('animated');
                    }, siblingIndex * 100);
                    
                    observer.unobserve(entry.target);
                }
            });
        }, observerOptions);

        animatedElements.forEach(el => observer.observe(el));
    }

    // ========================================
    // Smooth Scroll for Anchor Links
    // ========================================
    function setupSmoothScroll() {
        document.querySelectorAll('a[href^="#"]').forEach(anchor => {
            anchor.addEventListener('click', function(e) {
                const href = this.getAttribute('href');
                if (href === '#') return;
                
                const target = document.querySelector(href);
                if (target) {
                    e.preventDefault();
                    const navHeight = navbar.offsetHeight;
                    const targetPosition = target.offsetTop - navHeight - 20;
                    
                    window.scrollTo({
                        top: targetPosition,
                        behavior: 'smooth'
                    });

                    // Close mobile menu if open
                    closeMobileMenu();
                }
            });
        });
    }

    // ========================================
    // Active Navigation Link on Scroll
    // ========================================
    function setupActiveNavOnScroll() {
        const sections = document.querySelectorAll('section[id]');
        
        function updateActiveLink() {
            const scrollPos = window.scrollY + navbar.offsetHeight + 100;
            
            sections.forEach(section => {
                const sectionTop = section.offsetTop;
                const sectionHeight = section.offsetHeight;
                const sectionId = section.getAttribute('id');
                
                if (scrollPos >= sectionTop && scrollPos < sectionTop + sectionHeight) {
                    navLinks.forEach(link => {
                        link.classList.remove('active');
                        if (link.getAttribute('href') === `#${sectionId}`) {
                            link.classList.add('active');
                        }
                    });
                }
            });
        }

        window.addEventListener('scroll', updateActiveLink);
        updateActiveLink();
    }

    // ========================================
    // Logo Marquee Clone
    // ========================================
    function setupLogoMarquee() {
        const track = document.querySelector('.logo-track');
        if (track) {
            // Clone the logo items for seamless infinite scroll
            const items = track.innerHTML;
            track.innerHTML = items + items;
        }
    }

    // ========================================
    // Add transition styles to price amounts
    // ========================================
    function setupPriceAnimations() {
        priceAmounts.forEach(amount => {
            amount.style.transition = 'opacity 0.15s ease, transform 0.15s ease';
        });
    }

    // ========================================
    // Parallax Effect on Hero Background
    // ========================================
    function setupParallax() {
        const heroBg = document.querySelector('.hero-bg');
        if (!heroBg) return;

        window.addEventListener('scroll', () => {
            const scrolled = window.scrollY;
            heroBg.style.transform = `translateY(${scrolled * 0.3}px)`;
        });
    }

    // ========================================
    // Initialize
    // ========================================
    function init() {
        // Navbar scroll
        window.addEventListener('scroll', handleNavbarScroll);
        handleNavbarScroll();

        // Mobile menu
        if (mobileMenuBtn) {
            mobileMenuBtn.addEventListener('click', toggleMobileMenu);
        }

        // Close mobile menu when clicking outside
        document.addEventListener('click', (e) => {
            if (mobileMenu && mobileMenu.classList.contains('active')) {
                if (!mobileMenu.contains(e.target) && !mobileMenuBtn.contains(e.target)) {
                    closeMobileMenu();
                }
            }
        });

        // Pricing toggle
        if (pricingToggle) {
            pricingToggle.addEventListener('click', togglePricing);
        }

        // Setup animations
        setupScrollAnimations();
        setupSmoothScroll();
        setupActiveNavOnScroll();
        setupLogoMarquee();
        setupPriceAnimations();
        setupParallax();

        // Trigger initial animations for elements in view
        setTimeout(() => {
            animatedElements.forEach(el => {
                const rect = el.getBoundingClientRect();
                if (rect.top < window.innerHeight) {
                    el.classList.add('animated');
                }
            });
        }, 100);
    }

    // Run when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
