/**
 * NovaCode Landing Page JavaScript
 * Handles interactivity, animations, and dynamic content
 */

// ===================================
// Data: Features, Pricing, Testimonials
// ===================================

const features = [
  {
    icon: `<svg class="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"/>
    </svg>`,
    title: 'Lightning Fast',
    description: 'AI-powered completions appear instantly, keeping your flow state unbroken and your productivity at peak.'
  },
  {
    icon: `<svg class="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"/>
    </svg>`,
    title: 'Context Aware',
    description: 'Our AI understands your entire codebase, providing relevant suggestions based on your project\'s patterns.'
  },
  {
    icon: `<svg class="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"/>
    </svg>`,
    title: 'Multi-Language',
    description: 'Support for 50+ programming languages including Python, JavaScript, TypeScript, Go, Rust, and more.'
  },
  {
    icon: `<svg class="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"/>
    </svg>`,
    title: 'Secure by Design',
    description: 'Your code never leaves your machine. Privacy-first architecture with local processing options.'
  },
  {
    icon: `<svg class="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 5a1 1 0 011-1h14a1 1 0 011 1v2a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM4 13a1 1 0 011-1h6a1 1 0 011 1v6a1 1 0 01-1 1H5a1 1 0 01-1-1v-6zM16 13a1 1 0 011-1h2a1 1 0 011 1v6a1 1 0 01-1 1h-2a1 1 0 01-1-1v-6z"/>
    </svg>`,
    title: 'IDE Integration',
    description: 'Seamlessly works with VS Code, JetBrains, Vim, and more. One-click installation.'
  },
  {
    icon: `<svg class="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"/>
    </svg>`,
    title: 'Team Collaboration',
    description: 'Share custom prompts, code patterns, and AI configurations across your entire team.'
  }
];

const pricingPlans = [
  {
    name: 'Free',
    price: '$0',
    period: '/month',
    description: 'Perfect for trying out NovaCode on personal projects.',
    features: [
      '100 AI completions/day',
      'Basic code suggestions',
      'VS Code extension',
      'Community support'
    ],
    cta: 'Get Started',
    popular: false
  },
  {
    name: 'Pro',
    price: '$19',
    period: '/month',
    description: 'For professional developers who need more power.',
    features: [
      'Unlimited AI completions',
      'Advanced AI models',
      'All IDE integrations',
      'Priority support',
      'Code context analysis'
    ],
    cta: 'Start Pro Trial',
    popular: true
  },
  {
    name: 'Enterprise',
    price: 'Custom',
    period: '',
    description: 'For teams and organizations with advanced needs.',
    features: [
      'Everything in Pro',
      'Team management',
      'SSO & SAML',
      'Custom AI training',
      'Dedicated support'
    ],
    cta: 'Contact Sales',
    popular: false
  }
];

const testimonials = [
  {
    quote: 'NovaCode has completely transformed how I write code. The AI suggestions are incredibly accurate and help me ship features in half the time. It\'s like having a senior developer looking over my shoulder.',
    author: 'Sarah Chen',
    role: 'Senior Engineer @ Stripe',
    avatar: 'https://i.pravatar.cc/48?img=11'
  },
  {
    quote: 'We integrated NovaCode into our entire team\'s workflow. The context awareness is mind-blowing - it understands our codebase patterns and suggests exactly what we need. A game-changer for productivity.',
    author: 'Marcus Johnson',
    role: 'Tech Lead @ Vercel',
    avatar: 'https://i.pravatar.cc/48?img=12'
  }
];

// ===================================
// DOM Content Loaded
// ===================================

document.addEventListener('DOMContentLoaded', () => {
  // Initialize all components
  initNavbar();
  initMobileMenu();
  initScrollAnimations();
  initFeatureCards();
  initPricingCards();
  initTestimonials();
});

// ===================================
// Navbar Scroll Effect
// ===================================

function initNavbar() {
  const navbar = document.getElementById('navbar');
  let lastScrollY = window.scrollY;

  const handleScroll = () => {
    const currentScrollY = window.scrollY;
    
    if (currentScrollY > 50) {
      navbar.classList.add('scrolled');
    } else {
      navbar.classList.remove('scrolled');
    }
    
    lastScrollY = currentScrollY;
  };

  window.addEventListener('scroll', handleScroll, { passive: true });
  handleScroll(); // Initial check
}

// ===================================
// Mobile Menu Toggle
// ===================================

function initMobileMenu() {
  const menuBtn = document.getElementById('mobile-menu-btn');
  const mobileMenu = document.getElementById('mobile-menu');
  
  if (!menuBtn || !mobileMenu) return;

  menuBtn.addEventListener('click', () => {
    mobileMenu.classList.toggle('hidden');
    mobileMenu.classList.toggle('open');
  });

  // Close menu when clicking a link
  const mobileLinks = mobileMenu.querySelectorAll('a');
  mobileLinks.forEach(link => {
    link.addEventListener('click', () => {
      mobileMenu.classList.add('hidden');
      mobileMenu.classList.remove('open');
    });
  });
}

// ===================================
// Scroll-triggered Animations
// ===================================

function initScrollAnimations() {
  const observerOptions = {
    root: null,
    rootMargin: '0px',
    threshold: 0.1
  };

  const observer = new IntersectionObserver((entries) => {
    entries.forEach(entry => {
      if (entry.isIntersecting) {
        entry.target.classList.add('visible');
      }
    });
  }, observerOptions);

  // Observe all sections for fade-in animation
  document.querySelectorAll('section').forEach(section => {
    section.classList.add('fade-in-section');
    observer.observe(section);
  });
}

// ===================================
// Feature Cards
// ===================================

function initFeatureCards() {
  const grid = document.getElementById('features-grid');
  if (!grid) return;

  grid.innerHTML = features.map((feature, index) => `
    <div class="feature-card bg-surface p-8 rounded-2xl border border-border fade-in-section" style="animation-delay: ${index * 0.1}s;">
      <div class="w-14 h-14 bg-accent/20 rounded-xl flex items-center justify-center mb-6 text-accent">
        ${feature.icon}
      </div>
      <h3 class="text-xl font-bold mb-3">${feature.title}</h3>
      <p class="text-muted">${feature.description}</p>
    </div>
  `).join('');
}

// ===================================
// Pricing Cards
// ===================================

function initPricingCards() {
  const grid = document.getElementById('pricing-grid');
  if (!grid) return;

  grid.innerHTML = pricingPlans.map((plan, index) => `
    <div class="pricing-card bg-surface p-8 rounded-2xl border ${plan.popular ? 'border-2 border-accent relative' : 'border-border'} ${plan.popular ? 'popular' : ''} fade-in-section" style="animation-delay: ${index * 0.1}s;">
      ${plan.popular ? `
        <div class="absolute -top-4 left-1/2 -translate-x-1/2">
          <span class="bg-accent text-bg text-sm font-semibold px-4 py-1 rounded-full">Most Popular</span>
        </div>
      ` : ''}
      <div class="mb-6">
        <h3 class="text-lg font-semibold ${plan.popular ? 'text-accent' : 'text-muted'} mb-2">${plan.name}</h3>
        <div class="flex items-baseline">
          <span class="text-5xl font-bold">${plan.price}</span>
          <span class="text-muted ml-2">${plan.period}</span>
        </div>
      </div>
      <p class="text-muted mb-8">${plan.description}</p>
      <ul class="space-y-4 mb-8">
        ${plan.features.map(f => `
          <li class="flex items-center">
            <svg class="w-5 h-5 text-accent mr-3 flex-shrink-0" fill="currentColor" viewBox="0 0 24 24">
              <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z"/>
            </svg>
            <span>${f}</span>
          </li>
        `).join('')}
      </ul>
      <a href="#" class="block text-center ${plan.popular 
        ? 'bg-accent hover:bg-accent-hover text-bg font-semibold shadow-lg shadow-accent/25' 
        : 'border-2 border-border hover:border-accent'} px-6 py-3 rounded-xl transition-all">
        ${plan.cta}
      </a>
    </div>
  `).join('');
}

// ===================================
// Testimonials
// ===================================

function initTestimonials() {
  const grid = document.getElementById('testimonials-grid');
  if (!grid) return;

  grid.innerHTML = testimonials.map((testimonial, index) => `
    <div class="bg-surface p-8 rounded-2xl border border-border testimonial-quote fade-in-section" style="animation-delay: ${index * 0.1}s;">
      <svg class="w-10 h-10 text-accent/50 mb-4" fill="currentColor" viewBox="0 0 24 24">
        <path d="M14.017 21v-7.391c0-5.704 3.731-9.57 8.983-10.609l.995 2.151c-2.432.917-3.995 3.638-3.995 5.849h4v10h-9.983zm-14.017 0v-7.391c0-5.704 3.748-9.57 9-10.609l.996 2.151c-2.433.917-3.996 3.638-3.996 5.849h3.983v10h-9.983z"/>
      </svg>
      <p class="text-lg mb-6 leading-relaxed">${testimonial.quote}</p>
      <div class="flex items-center">
        <img src="${testimonial.avatar}" alt="${testimonial.author}" class="w-12 h-12 rounded-full mr-4">
        <div>
          <div class="font-semibold">${testimonial.author}</div>
          <div class="text-sm text-muted">${testimonial.role}</div>
        </div>
      </div>
    </div>
  `).join('');
}

// ===================================
// Smooth Scroll for Anchor Links
// ===================================

document.querySelectorAll('a[href^="#"]').forEach(anchor => {
  anchor.addEventListener('click', function(e) {
    const href = this.getAttribute('href');
    if (href === '#') return;
    
    e.preventDefault();
    const target = document.querySelector(href);
    if (target) {
      target.scrollIntoView({
        behavior: 'smooth',
        block: 'start'
      });
    }
  });
});
