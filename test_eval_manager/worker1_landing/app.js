// NovaCode Labs - Main Application
import { initNavbar } from './components/navbar.js';
import { initHero } from './components/hero.js';
import { initFeatures } from './components/features.js';
import { initPricing } from './components/pricing.js';
import { initTestimonials } from './components/testimonials.js';
import { initContact } from './components/contact.js';
import { initFooter } from './components/footer.js';

// Initialize all components
document.addEventListener('DOMContentLoaded', () => {
    initNavbar();
    initHero();
    initFeatures();
    initPricing();
    initTestimonials();
    initContact();
    initFooter();
    
    // Smooth scroll for anchor links
    document.querySelectorAll('a[href^="#"]').forEach(anchor => {
        anchor.addEventListener('click', function(e) {
            e.preventDefault();
            const target = document.querySelector(this.getAttribute('href'));
            if (target) {
                target.scrollIntoView({ behavior: 'smooth' });
            }
        });
    });
    
    console.log('NovaCode Labs initialized successfully');
});
