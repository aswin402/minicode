/**
 * minicode Landing Page - JavaScript
 * Handles animations, terminal typing effect, and DAG visualization
 */

document.addEventListener('DOMContentLoaded', () => {
    initTerminalAnimation();
    initDAGVisualization();
    initCounterAnimations();
    initScrollAnimations();
});

/**
 * Terminal Typing Animation
 * Simulates terminal output being typed in real-time
 */
function initTerminalAnimation() {
    const terminal = document.getElementById('terminal');
    if (!terminal) return;

    const lines = terminal.querySelectorAll('.terminal-body code');
    if (!lines.length) return;

    const code = lines[0];
    const originalContent = code.innerHTML;
    
    // Reset and animate on scroll into view
    const observer = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                animateTerminal(code, originalContent);
                observer.unobserve(entry.target);
            }
        });
    }, { threshold: 0.5 });

    observer.observe(terminal);
}

function animateTerminal(code, content) {
    code.innerHTML = '';
    const lines = content.split('\n');
    let lineIndex = 0;

    function typeLine() {
        if (lineIndex < lines.length) {
            const line = lines[lineIndex];
            const span = document.createElement('span');
            span.innerHTML = line;
            code.appendChild(span);
            if (lineIndex < lines.length - 1) {
                code.appendChild(document.createTextNode('\n'));
            }
            lineIndex++;
            setTimeout(typeLine, 200 + Math.random() * 100);
        }
    }

    typeLine();
}

/**
 * DAG Visualization
 * Renders an animated directed acyclic graph for task orchestration
 */
function initDAGVisualization() {
    const dagCanvas = document.getElementById('dagCanvas');
    if (!dagCanvas) return;

    const svg = dagCanvas.querySelector('.dag-svg');
    if (!svg) return;

    // Define DAG nodes
    const nodes = [
        { id: 'root', x: 400, y: 50, label: 'Task', status: 'completed' },
        { id: 'analyze', x: 200, y: 130, label: 'Analyze', status: 'completed' },
        { id: 'plan', x: 400, y: 130, label: 'Plan', status: 'completed' },
        { id: 'execute', x: 600, y: 130, label: 'Execute', status: 'completed' },
        { id: 'parse', x: 100, y: 210, label: 'Parse', status: 'active' },
        { id: 'search', x: 300, y: 210, label: 'Search', status: 'active' },
        { id: 'verify', x: 500, y: 210, label: 'Verify', status: 'pending' },
        { id: 'write', x: 700, y: 210, label: 'Write', status: 'pending' },
        { id: 'test', x: 400, y: 290, label: 'Test', status: 'pending' },
    ];

    // Define edges
    const edges = [
        { from: 'root', to: 'analyze' },
        { from: 'root', to: 'plan' },
        { from: 'root', to: 'execute' },
        { from: 'analyze', to: 'parse' },
        { from: 'analyze', to: 'search' },
        { from: 'plan', to: 'search' },
        { from: 'execute', to: 'verify' },
        { from: 'execute', to: 'write' },
        { from: 'parse', to: 'test' },
        { from: 'search', to: 'test' },
        { from: 'verify', to: 'test' },
        { from: 'write', to: 'test' },
    ];

    // Clear existing content
    svg.innerHTML = '';

    // Add defs for gradients and filters
    const defs = document.createElementNS('http://www.w3.org/2000/svg', 'defs');
    defs.innerHTML = `
        <linearGradient id="nodeGradient" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" style="stop-color:#00d4ff;stop-opacity:0.3" />
            <stop offset="100%" style="stop-color:#ff00aa;stop-opacity:0.3" />
        </linearGradient>
        <linearGradient id="edgeGradient" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" style="stop-color:#00d4ff;stop-opacity:0.5" />
            <stop offset="100%" style="stop-color:#ff00aa;stop-opacity:0.5" />
        </linearGradient>
        <filter id="glow">
            <feGaussianBlur stdDeviation="3" result="coloredBlur"/>
            <feMerge>
                <feMergeNode in="coloredBlur"/>
                <feMergeNode in="SourceGraphic"/>
            </feMerge>
        </filter>
    `;
    svg.appendChild(defs);

    // Draw edges
    edges.forEach(edge => {
        const fromNode = nodes.find(n => n.id === edge.from);
        const toNode = nodes.find(n => n.id === edge.to);
        if (fromNode && toNode) {
            const line = document.createElementNS('http://www.w3.org/2000/svg', 'line');
            line.setAttribute('x1', fromNode.x);
            line.setAttribute('y1', fromNode.y + 25);
            line.setAttribute('x2', toNode.x);
            line.setAttribute('y2', toNode.y - 25);
            line.setAttribute('stroke', 'url(#edgeGradient)');
            line.setAttribute('stroke-width', '2');
            line.setAttribute('class', 'dag-edge');
            svg.appendChild(line);

            // Arrow marker
            const arrow = document.createElementNS('http://www.w3.org/2000/svg', 'polygon');
            const angle = Math.atan2(toNode.y - fromNode.y, toNode.x - fromNode.x);
            const arrowX = toNode.x - 25 * Math.cos(angle);
            const arrowY = toNode.y - 25 * Math.sin(angle);
            const arrowSize = 8;
            arrow.setAttribute('points', `${arrowX},${arrowY} ${arrowX - arrowSize * Math.cos(angle - Math.PI / 6)},${arrowY - arrowSize * Math.sin(angle - Math.PI / 6)} ${arrowX - arrowSize * Math.cos(angle + Math.PI / 6)},${arrowY - arrowSize * Math.sin(angle + Math.PI / 6)}`);
            arrow.setAttribute('fill', '#ff00aa');
            arrow.setAttribute('opacity', '0.5');
            svg.appendChild(arrow);
        }
    });

    // Draw nodes
    nodes.forEach((node, index) => {
        const g = document.createElementNS('http://www.w3.org/2000/svg', 'g');
        g.setAttribute('class', `dag-node ${node.status}`);
        g.style.opacity = '0';
        g.style.transform = `translate(0, -20px)`;

        // Circle
        const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
        circle.setAttribute('cx', node.x);
        circle.setAttribute('cy', node.y);
        circle.setAttribute('r', '25');
        circle.setAttribute('fill', 'url(#nodeGradient)');
        circle.setAttribute('stroke', getStatusColor(node.status));
        circle.setAttribute('stroke-width', '2');
        if (node.status === 'active') {
            circle.setAttribute('filter', 'url(#glow)');
        }
        g.appendChild(circle);

        // Label
        const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
        text.setAttribute('x', node.x);
        text.setAttribute('y', node.y + 40);
        text.setAttribute('text-anchor', 'middle');
        text.setAttribute('fill', '#8892a0');
        text.setAttribute('font-size', '12');
        text.textContent = node.label;
        g.appendChild(text);

        svg.appendChild(g);

        // Animate in
        setTimeout(() => {
            g.style.transition = 'opacity 0.4s ease-out, transform 0.4s ease-out';
            g.style.opacity = '1';
            g.style.transform = 'translate(0, 0)';
        }, index * 150);
    });
}

function getStatusColor(status) {
    switch (status) {
        case 'completed': return '#00ff88';
        case 'active': return '#00d4ff';
        case 'pending': return '#8892a0';
        default: return '#8892a0';
    }
}

/**
 * Counter Animations
 * Animates numbers counting up when scrolled into view
 */
function initCounterAnimations() {
    const counters = document.querySelectorAll('.stat-value');
    if (!counters.length) return;

    const observer = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                animateCounter(entry.target);
                observer.unobserve(entry.target);
            }
        });
    }, { threshold: 0.5 });

    counters.forEach(counter => observer.observe(counter));
}

function animateCounter(element) {
    const target = parseInt(element.dataset.value, 10);
    const duration = 2000;
    const startTime = performance.now();

    function update(currentTime) {
        const elapsed = currentTime - startTime;
        const progress = Math.min(elapsed / duration, 1);
        
        // Easing function (ease-out)
        const easeOut = 1 - Math.pow(1 - progress, 3);
        const current = Math.floor(target * easeOut);
        
        element.textContent = current.toLocaleString();

        if (progress < 1) {
            requestAnimationFrame(update);
        } else {
            element.textContent = target.toLocaleString();
        }
    }

    requestAnimationFrame(update);
}

/**
 * Scroll Animations
 * Adds fade-in animations as sections scroll into view
 */
function initScrollAnimations() {
    const sections = document.querySelectorAll('.section');
    
    const observer = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                entry.target.classList.add('visible');
            }
        });
    }, { threshold: 0.1, rootMargin: '0px 0px -50px 0px' });

    sections.forEach(section => {
        section.style.opacity = '0';
        section.style.transform = 'translateY(30px)';
        section.style.transition = 'opacity 0.6s ease-out, transform 0.6s ease-out';
        observer.observe(section);
    });

    // Also observe cards and grids
    const animatedElements = document.querySelectorAll('.stat-card, .feature-card, .agent-card, .tool-category, .testimonial-card');
    animatedElements.forEach((el, index) => {
        el.style.opacity = '0';
        el.style.transform = 'translateY(20px)';
        el.style.transition = 'opacity 0.5s ease-out, transform 0.5s ease-out';
        el.style.transitionDelay = `${index * 0.1}s`;
        observer.observe(el);
    });
}

// Add visible class handling
const style = document.createElement('style');
style.textContent = `
    .visible {
        opacity: 1 !important;
        transform: translateY(0) !important;
    }
`;
document.head.appendChild(style);
