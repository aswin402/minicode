#![allow(clippy::all, unused)]

use crate::tools::onpkg::stacks::{Stack, StackFile};

pub fn pern() -> Stack {
    Stack {
        hooks: vec![],
        name: "pern".into(),
        runtime: "bun".into(),
        description: "PERN Stack - Express + React + PostgreSQL + Prisma + TypeScript (Monorepo)".into(),
        packages: vec![
            "express".into(),
            "cors".into(),
            "dotenv".into(),
            "bcryptjs".into(),
            "jsonwebtoken".into(),
            "react".into(),
            "react-dom".into(),
            "@prisma/client".into(),
            "@prisma/adapter-pg".into(),
            "pg".into(),
        ],
        dev_packages: vec![
            "typescript".into(),
            "@types/node".into(),
            "@types/express".into(),
            "@types/cors".into(),
            "@types/bcryptjs".into(),
            "@types/jsonwebtoken".into(),
            "@types/react".into(),
            "@types/react-dom".into(),
            "@types/pg".into(),
            "tsx".into(),
            "nodemon".into(),
            "prisma".into(),
            "vite".into(),
            "@vitejs/plugin-react".into(),
            "tailwindcss".into(),
            "postcss".into(),
            "autoprefixer".into(),
        ],
        transitive_packages: vec![],
        files: vec![
            // Backend
            StackFile {
                path: "backend/package.json".into(),
                content: r##"{
  "name": "pern-backend",
  "version": "1.0.0",
  "scripts": {
    "dev": "tsx --watch src/app.ts",
    "start": "tsx src/app.ts",
    "prisma:generate": "prisma generate",
    "prisma:db-push": "prisma db push",
    "prisma:studio": "prisma studio"
  },
  "dependencies": {
    "express": "",
    "cors": "",
    "dotenv": "",
    "bcryptjs": "",
    "jsonwebtoken": "",
    "@prisma/client": "",
    "@prisma/adapter-pg": "",
    "pg": ""
  },
  "devDependencies": {
    "typescript": "",
    "@types/node": "",
    "@types/express": "",
    "@types/cors": "",
    "@types/bcryptjs": "",
    "@types/jsonwebtoken": "",
    "@types/pg": "",
    "tsx": "",
    "nodemon": "",
    "prisma": ""
  }
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "backend/prisma/schema.prisma".into(),
                content: r##"generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

model User {
  id        String   @id @default(cuid())
  email     String   @unique
  name      String?
  password  String
  createdAt DateTime @default(now())
  updatedAt DateTime @updatedAt
}
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "backend/tsconfig.json".into(),
                content: r##"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "commonjs",
    "lib": ["ES2022"],
    "outDir": "./dist",
    "rootDir": "./src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true
  },
  "include": ["src/**/*", "prisma/**/*.ts"],
  "exclude": ["node_modules", "dist"]
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "backend/src/app.ts".into(),
                content: r##"import express from 'express';
import cors from 'cors';
import dotenv from 'dotenv';
import { PrismaClient } from '@prisma/client';
import userRoutes from './routes/userRoutes';

dotenv.config();

export const prisma = new PrismaClient();

const app = express();
const PORT = process.env.PORT || 5000;

app.use(cors());
app.use(express.json());

app.use('/api/users', userRoutes);

app.get('/', (req, res) => {
  res.json({ message: 'PERN Backend API' });
});

app.listen(PORT, () => {
  console.log(`🚀 Server running on port ${PORT}`);
});

export default app;"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "backend/src/routes/userRoutes.ts".into(),
                content: r##"import express from 'express';
import { prisma } from '../app';

const router = express.Router();

router.get('/', async (req, res) => {
  const users = await prisma.user.findMany();
  res.json(users);
});

router.post('/', async (req, res) => {
  const user = await prisma.user.create({
    data: req.body
  });
  res.status(201).json(user);
});

export default router;"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "backend/.env".into(),
                content: "DATABASE_URL=\"postgresql://postgres:password@localhost:5432/pern\"\nPORT=5000\nJWT_SECRET=your_jwt_secret".into(), binary_content: None,
            },

            // Frontend (same as PERN but proxy to 5000)
            StackFile {
                path: "frontend/vite.config.ts".into(),
                content: r##"import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': 'http://localhost:5000'
    }
  }
})"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "frontend/index.html".into(),
                content: r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>PERN App</title>
</head>
<body>
  <div id="root"></div>
  <script type="module" src="/src/main.tsx"></script>
</body>
</html>"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "frontend/src/App.tsx".into(),
                content: r##"import { useEffect, useState } from 'react';

interface User {
  id: string;
  name?: string;
  email: string;
  createdAt: string;
}

export default function App() {
  const [users, setUsers] = useState<User[]>([]);

  useEffect(() => {
    fetch('/api/users')
      .then(res => res.json())
      .then(setUsers);
  }, []);

  return (
    <div className="min-h-screen bg-gradient-to-br from-emerald-900 via-teal-900 to-cyan-800 text-white p-8">
      <div className="max-w-4xl mx-auto">
        <h1 className="text-5xl font-black mb-12 bg-gradient-to-r from-white to-gray-300 bg-clip-text text-transparent">
          PERN Stack
        </h1>
        <div className="grid gap-4">
          {users.map(user => (
            <div key={user.id} className="bg-white/10 backdrop-blur-xl p-6 rounded-2xl border border-white/20 hover:scale-[1.02] transition-transform">
              <h3 className="text-xl font-bold">{user.name || 'Anonymous'}</h3>
              <p className="text-gray-300">{user.email}</p>
              <p className="text-xs text-gray-500 mt-1">{new Date(user.createdAt).toLocaleDateString()}</p>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}"##.into(),
                binary_content: None,
            },

            // Root monorepo files
            StackFile {
                path: "package.json".into(),
                content: r##"{
  "name": "pern-monorepo",
  "workspaces": ["backend", "frontend"],
  "scripts": {
    "dev:backend": "cd backend && bun prisma:generate && bun prisma:db-push && bun dev",
    "dev:frontend": "cd frontend && bun dev",
    "dev": "concurrently \"bun dev:backend\" \"bun dev:frontend\""
  },
  "devDependencies": {
    "concurrently": ""
  }
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "docker-compose.yml".into(),
                content: r##"version: '3.8'
services:
  postgres:
    image: postgres:16
    ports:
      - '5432:5432'
    environment:
      POSTGRES_DB: pern
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: password
    volumes:
      - postgres_data:/var/lib/postgresql/data

volumes:
  postgres_data:
"##.into(),
                binary_content: None,
            },
        ],
    }
}
