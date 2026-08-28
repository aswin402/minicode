#![allow(clippy::all, unused)]

use crate::tools::onpkg::stacks::{Stack, StackFile};

pub fn mern() -> Stack {
    Stack {
        hooks: vec![],
        name: "mern".into(),
        runtime: "bun".into(),
        description: "MERN Stack - Express + React + MongoDB + TypeScript (Monorepo)".into(),
        packages: vec![
            "express".into(),
            "cors".into(),
            "mongoose".into(),
            "dotenv".into(),
            "bcryptjs".into(),
            "jsonwebtoken".into(),
            "react".into(),
            "react-dom".into(),
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
            "tsx".into(),
            "nodemon".into(),
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
  "name": "mern-backend",
  "version": "1.0.0",
  "scripts": {
    "dev": "tsx --watch src/app.ts",
    "start": "tsx src/app.ts"
  },
  "dependencies": {
    "express": "",
    "cors": "",
    "mongoose": "",
    "dotenv": "",
    "bcryptjs": "",
    "jsonwebtoken": ""
  },
  "devDependencies": {
    "typescript": "",
    "@types/node": "",
    "@types/express": "",
    "@types/cors": "",
    "@types/bcryptjs": "",
    "@types/jsonwebtoken": "",
    "tsx": "",
    "nodemon": ""
  }
}"##.into(),
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
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "backend/src/app.ts".into(),
                content: r##"import express from 'express';
import cors from 'cors';
import dotenv from 'dotenv';
import userRoutes from './routes/userRoutes';

dotenv.config();

const app = express();
const PORT = process.env.PORT || 5000;

app.use(cors());
app.use(express.json());

app.use('/api/users', userRoutes);

app.get('/', (req, res) => {
  res.json({ message: 'MERN Backend API' });
});

app.listen(PORT, () => {
  console.log(`🚀 Server running on port ${PORT}`);
});

export default app;"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "backend/src/models/User.ts".into(),
                content: r##"import mongoose from 'mongoose';

const userSchema = new mongoose.Schema({
  name: { type: String, required: true },
  email: { type: String, required: true, unique: true },
  password: { type: String, required: true },
}, { timestamps: true });

export default mongoose.model('User', userSchema);"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "backend/src/routes/userRoutes.ts".into(),
                content: r##"import express from 'express';
import User from '../models/User';

const router = express.Router();

router.get('/', async (req, res) => {
  const users = await User.find();
  res.json(users);
});

router.post('/', async (req, res) => {
  const user = new User(req.body);
  await user.save();
  res.status(201).json(user);
});

export default router;"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "backend/.env".into(),
                content: "MONGODB_URI=mongodb://localhost:27017/mern\nPORT=5000\nJWT_SECRET=your_jwt_secret".into(), binary_content: None,
            },

            // Frontend (same as react_vite_full)
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
  <title>MERN App</title>
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
  _id: string;
  name: string;
  email: string;
}

export default function App() {
  const [users, setUsers] = useState<User[]>([]);

  useEffect(() => {
    fetch('/api/users')
      .then(res => res.json())
      .then(setUsers);
  }, []);

  return (
    <div className="min-h-screen bg-gradient-to-br from-indigo-900 via-purple-900 to-pink-800 text-white p-8">
      <div className="max-w-4xl mx-auto">
        <h1 className="text-5xl font-black mb-12 bg-gradient-to-r from-white to-gray-300 bg-clip-text text-transparent">
          MERN Stack
        </h1>
        <div className="grid gap-4">
          {users.map(user => (
            <div key={user._id} className="bg-white/10 backdrop-blur-xl p-6 rounded-2xl border border-white/20">
              <h3 className="text-xl font-bold">{user.name}</h3>
              <p className="text-gray-300">{user.email}</p>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "frontend/src/main.tsx".into(),
                content: r##"import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App.tsx'
import './index.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "frontend/tailwind.config.js".into(),
                content: r##"/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: { extend: {} },
  plugins: [],
}"##.into(),
                binary_content: None,
            },

            // Root files
            StackFile {
                path: "package.json".into(),
                content: r##"{
  "name": "mern-monorepo",
  "workspaces": ["backend", "frontend"],
  "scripts": {
    "dev:backend": "cd backend && bun dev",
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
  mongo:
    image: mongo:7
    ports:
      - '27017:27017'
    environment:
      MONGO_INITDB_ROOT_USERNAME: admin
      MONGO_INITDB_ROOT_PASSWORD: password
"##.into(),
                binary_content: None,
            },
        ],
    }
}
