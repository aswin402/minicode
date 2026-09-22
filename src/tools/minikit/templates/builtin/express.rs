#![allow(clippy::all, unused)]

use crate::tools::minikit::stacks::{Stack, StackFile};

pub fn express_api() -> Stack {
    Stack {
        hooks: vec![],
        name: "express-api".into(),
        runtime: "bun".into(),
        description: "Express 5 + TypeScript + Zod schema validation API".into(),
        packages: vec![
            "express".into(),
            "cors".into(),
            "zod".into(),
            "dotenv".into(),
        ],
        dev_packages: vec![
            "typescript".into(),
            "@types/express".into(),
            "@types/cors".into(),
            "@types/node".into(),
            "tsx".into(),
        ],
        transitive_packages: vec![],
        files: vec![
            StackFile {
                path: "package.json".into(),
                content: r##"{
  "name": "express-api",
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "dev": "tsx watch src/index.ts",
    "build": "tsc",
    "start": "node dist/index.js"
  },
  "dependencies": {
    "express": "^5.0.0",
    "cors": "^2.8.5",
    "zod": "^3.23.8",
    "dotenv": "^16.4.5"
  },
  "devDependencies": {
    "typescript": "^5.6.3",
    "@types/express": "^5.0.0",
    "@types/cors": "^2.8.17",
    "@types/node": "^22.9.0",
    "tsx": "^4.19.2"
  }
}
"##
                .into(),
                binary_content: None,
            },
            StackFile {
                path: "tsconfig.json".into(),
                content: r##"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "lib": ["ES2022"],
    "outDir": "./dist",
    "rootDir": "./src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true
  },
  "include": ["src/**/*"]
}
"##
                .into(),
                binary_content: None,
            },
            StackFile {
                path: "src/index.ts".into(),
                content: r##"import express from "express";
import cors from "cors";
import dotenv from "dotenv";

dotenv.config();

const app = express();
const PORT = process.env.PORT || 3000;

app.use(cors());
app.use(express.json());

app.get("/health", (req, res) => {
  res.json({ status: "healthy", timestamp: new Date().toISOString() });
});

app.listen(PORT, () => {
  console.log(`Server listening on http://localhost:${PORT}`);
});
"##
                .into(),
                binary_content: None,
            },
            StackFile {
                path: "README.md".into(),
                content: r##"# Express API Starter

Scaffolded with minicode + MiniKit.

## Getting Started

```bash
bun install  # or npm install
bun dev      # starts tsx watcher on http://localhost:3000
```
"##
                .into(),
                binary_content: None,
            },
        ],
    }
}
