#![allow(clippy::all, unused)]

use crate::tools::minikit::stacks::{Stack, StackFile};

pub fn static_website() -> Stack {
    Stack {
        hooks: vec![],
        name: "static-website".into(),
        runtime: "bun".into(),
        description: "Minimal modern HTML5 + CSS3 + JS static website".into(),
        packages: vec![],
        dev_packages: vec![],
        transitive_packages: vec![],
        files: vec![
            StackFile {
                path: "index.html".into(),
                content: r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Modern Static Website</title>
  <link rel="stylesheet" href="style.css" />
</head>
<body>
  <main class="container">
    <h1>Welcome to Your Website</h1>
    <p>Built with lightweight, standards-compliant HTML5, CSS3, and JavaScript.</p>
    <button id="cta-btn">Click Me</button>
  </main>
  <script src="app.js"></script>
</body>
</html>
"##
                .into(),
                binary_content: None,
            },
            StackFile {
                path: "style.css".into(),
                content: r##"* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  display: grid;
  place-items: center;
  min-height: 100vh;
  background: #0f172a;
  color: #e2e8f0;
}

.container {
  text-align: center;
  max-width: 600px;
  padding: 2rem;
}

h1 {
  font-size: 2.5rem;
  margin-bottom: 1rem;
  background: linear-gradient(135deg, #38bdf8, #818cf8);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
}

p {
  font-size: 1.125rem;
  color: #94a3b8;
  margin-bottom: 1.5rem;
}

button {
  background: #6366f1;
  color: white;
  border: none;
  padding: 0.75rem 1.5rem;
  font-size: 1rem;
  border-radius: 0.5rem;
  cursor: pointer;
  transition: background 0.2s ease;
}

button:hover {
  background: #4f46e5;
}
"##
                .into(),
                binary_content: None,
            },
            StackFile {
                path: "app.js".into(),
                content: r##"document.addEventListener('DOMContentLoaded', () => {
  const btn = document.getElementById('cta-btn');
  if (btn) {
    btn.addEventListener('click', () => {
      alert('Hello from MiniKit static website!');
    });
  }
});
"##
                .into(),
                binary_content: None,
            },
            StackFile {
                path: "README.md".into(),
                content: r##"# Static Website

Lightweight static website scaffolded with minicode + MiniKit.

Open `index.html` in any modern web browser or serve with `bun x serve` or `npx serve`.
"##
                .into(),
                binary_content: None,
            },
        ],
    }
}
