#![allow(clippy::all, unused)]

use crate::tools::onpkg::stacks::{Stack, StackFile};

pub fn next_template() -> Stack {
    Stack {
        hooks: vec![],
        name: "next-template".into(),
        runtime: "bun".into(),
        description: "Upgraded Next.js 16 + Bun + Tailwind CSS v4 + Prisma 7 + Professional Backend".into(),
        packages: vec![
            "@hookform/resolvers".into(),
            "@tanstack/react-query".into(),
            "axios".into(),
            "react-hook-form".into(),
            "zod".into(),
            "zustand".into(),
            "@prisma/client".into(),
            "bcryptjs".into(),
            "jsonwebtoken".into(),
            "superjson".into(),
            "next-themes".into(),
            "pino".into(),
            "ioredis".into(),
            "nodemailer".into(),
            "node-cron".into(),
            "@aws-sdk/client-s3".into(),
            "@aws-sdk/s3-request-presigner".into(),
            "cors".into(),
            "class-variance-authority".into(),
            "clsx".into(),
            "lucide-react".into(),
            "next".into(),
            "radix-ui".into(),
            "react".into(),
            "react-dom".into(),
            "shadcn".into(),
            "tailwind-merge".into(),
            "tw-animate-css".into(),
        ],
        dev_packages: vec![
            "prisma".into(),
            "@types/bcryptjs".into(),
            "@types/jsonwebtoken".into(),
            "typescript".into(),
            "@types/node".into(),
            "@types/react".into(),
            "@types/react-dom".into(),
            "eslint".into(),
            "eslint-config-next".into(),
            "tailwindcss".into(),
            "@tailwindcss/postcss".into(),
            "@types/pg".into(),
            "pino-pretty".into(),
            "@types/nodemailer".into(),
            "@types/node-cron".into(),
            "@types/cors".into(),
        ],
        transitive_packages: vec![],
        files: vec![
            StackFile {
                path: "next.config.ts".into(),
                content: r###"import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  logging: {
    incomingRequests: false,
  },
};

export default nextConfig;
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "next-env.d.ts".into(),
                content: r###"/// <reference types="next" />
/// <reference types="next/image-types/global" />
import "./.next/types/routes.d.ts";

// NOTE: This file should not be edited
// see https://nextjs.org/docs/app/api-reference/config/typescript for more information.
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "README.md".into(),
                content: r###"# Onpkg Next.js Full-Stack Starter Template 🚀

Welcome to your upgraded, high-performance project template built with the latest modern web technologies. This project is optimized for speed, security, and developer experience.

## ✨ Features

- **Framework**: [Next.js 16 (App Router)](https://nextjs.org/) utilizing React Canary features (e.g., Server Components, Async Params, Server Functions).
- **Runtime & Bundler**: [Bun](https://bun.sh/) for ultra-fast package install, script runs, and hot reload.
- **Styling**: [Tailwind CSS v4](https://tailwindcss.com/) with native OKLCH colors and cascading layers.
- **ORM & Database**: [Prisma v7](https://www.prisma.io/) with native JavaScript driver adapters (`@prisma/adapter-pg` and `pg` pool) for a 90% smaller engine bundle.
- **UI System**: Pre-configured [Shadcn UI](https://ui.shadcn.com/) components.
- **State Management**: Hydration-safe [Zustand](https://docs.pmnd.rs/zustand) stores.
- **Data Fetching**: [TanStack Query v5 (React Query)](https://tanstack.com/query) client provider and cached queries.
- **Validation**: [Zod](https://zod.dev/) type-safe schemas.
- **Authentication**: Secure token-based session handling with `bcryptjs` password hashing, JSON Web Tokens (JWT), and HTTP-only cookies.
- **Structured Logging**: Dual-mode logger (colored server CLI logs + clean group-collapsed browser console entries).

---

## 📂 Documentation

Detailed manuals are available in the `docs/` directory:

1. [Database Setup & Prisma 7 Guide](docs/PRISMA.md) - Deep dive into database config, driver adapters, and schema structure.
2. [Architecture & Auth Layout](docs/ARCHITECTURE.md) - Explains folder hierarchy, global state, React Query hooks, and security flows.
3. [Structured Logging with Pino](docs/LOGGING.md) - High-performance structured logging.
4. [Redis & API Rate Limiting](docs/REDIS.md) - Setup for Redis caching and sliding-window rate limiters.
5. [Transactional SMTP Mailer](docs/MAILER.md) - Dispatching HTML emails using Nodemailer.
6. [Object File Storage (S3 & R2)](docs/STORAGE.md) - Object uploads and client presigned URLs.
7. [Scheduled Background Tasks (Cron)](docs/CRON.md) - Background cron registers utilizing Next.js instrumentation.

---

## 🛠️ Getting Started

### 1. Requirements
Ensure you have [Bun](https://bun.sh/) installed:
```bash
curl -fsSL https://bun.sh/install | bash
```

### 2. Installation
Install project dependencies:
```bash
bun install
```

### 3. Database & Environment Setup
Open `.env` in the root directory to confirm the default PostgreSQL database credentials match your docker setup:
```env
DATABASE_URL="postgresql://postgres:postgres@localhost:5432/onpkg_db?schema=public"
```

Spin up the local PostgreSQL database using Docker Compose:
```bash
docker-compose up -d
```

### 4. Running Database Migrations
Initialize database tables using Prisma CLI scripts:
```bash
bun run db:migrate
```

Re-generate client bindings and seed mock users/posts:
```bash
bun run db:generate
bun run db:seed
```

### 5. Running the Application
Spin up the hot-reload dev server:
```bash
bun run dev
```

Your app will be live at [http://localhost:3000](http://localhost:3000).

---

## 📦 Script Directory

All primary commands are run via Bun:

| Command | Action |
| :--- | :--- |
| `bun run dev` | Starts the Next.js development server |
| `bun run build` | Builds the production bundle |
| `bun run start` | Runs the built production bundle |
| `bun run lint` | Runs ESLint check |
| `bun run db:migrate` | Runs database migrations |
| `bun run db:generate` | Re-generates Prisma type-safe client |
| `bun run db:seed` | Resets database and seeds mock data |
| `bun run db:studio` | Opens interactive database panel in browser |

---

## 📜 License
MIT
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "tsconfig.json".into(),
                content: r###"{
  "compilerOptions": {
    "target": "ES2017",
    "lib": ["dom", "dom.iterable", "esnext"],
    "allowJs": true,
    "skipLibCheck": true,
    "strict": true,
    "noEmit": true,
    "esModuleInterop": true,
    "module": "esnext",
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "jsx": "react-jsx",
    "incremental": true,
    "plugins": [
      {
        "name": "next"
      }
    ],
    "paths": {
      "@/*": ["./*"]
    }
  },
  "include": [
    "next-env.d.ts",
    "**/*.ts",
    "**/*.tsx",
    ".next/types/**/*.ts",
    ".next/dev/types/**/*.ts",
    "**/*.mts"
  ],
  "exclude": ["node_modules"]
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "components/ui/card.tsx".into(),
                content: r###"import * as React from "react"

import { cn } from "@/lib/utils"

function Card({
  className,
  size = "default",
  ...props
}: React.ComponentProps<"div"> & { size?: "default" | "sm" }) {
  return (
    <div
      data-slot="card"
      data-size={size}
      className={cn(
        "group/card flex flex-col gap-(--card-spacing) overflow-hidden rounded-xl bg-card py-(--card-spacing) text-sm text-card-foreground ring-1 ring-foreground/10 [--card-spacing:--spacing(4)] has-data-[slot=card-footer]:pb-0 has-[>img:first-child]:pt-0 data-[size=sm]:[--card-spacing:--spacing(3)] data-[size=sm]:has-data-[slot=card-footer]:pb-0 *:[img:first-child]:rounded-t-xl *:[img:last-child]:rounded-b-xl",
        className
      )}
      {...props}
    />
  )
}

function CardHeader({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-header"
      className={cn(
        "group/card-header @container/card-header grid auto-rows-min items-start gap-1 rounded-t-xl px-(--card-spacing) has-data-[slot=card-action]:grid-cols-[1fr_auto] has-data-[slot=card-description]:grid-rows-[auto_auto] [.border-b]:pb-(--card-spacing)",
        className
      )}
      {...props}
    />
  )
}

function CardTitle({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-title"
      className={cn(
        "font-heading text-base leading-snug font-medium group-data-[size=sm]/card:text-sm",
        className
      )}
      {...props}
    />
  )
}

function CardDescription({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-description"
      className={cn("text-sm text-muted-foreground", className)}
      {...props}
    />
  )
}

function CardAction({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-action"
      className={cn(
        "col-start-2 row-span-2 row-start-1 self-start justify-self-end",
        className
      )}
      {...props}
    />
  )
}

function CardContent({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-content"
      className={cn("px-(--card-spacing)", className)}
      {...props}
    />
  )
}

function CardFooter({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-footer"
      className={cn(
        "flex items-center rounded-b-xl border-t bg-muted/50 p-(--card-spacing)",
        className
      )}
      {...props}
    />
  )
}

export {
  Card,
  CardHeader,
  CardFooter,
  CardTitle,
  CardAction,
  CardDescription,
  CardContent,
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "components/ui/button.tsx".into(),
                content: r###"import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Slot } from "radix-ui"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "group/button inline-flex shrink-0 items-center justify-center rounded-lg border border-transparent bg-clip-padding text-sm font-medium whitespace-nowrap transition-all outline-none select-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 active:not-aria-[haspopup]:translate-y-px disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-primary/80",
        outline:
          "border-border bg-background hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground dark:border-input dark:bg-input/30 dark:hover:bg-input/50",
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-[color-mix(in_oklch,var(--secondary),var(--foreground)_5%)] aria-expanded:bg-secondary aria-expanded:text-secondary-foreground",
        ghost:
          "hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground dark:hover:bg-muted/50",
        destructive:
          "bg-destructive/10 text-destructive hover:bg-destructive/20 focus-visible:border-destructive/40 focus-visible:ring-destructive/20 dark:bg-destructive/20 dark:hover:bg-destructive/30 dark:focus-visible:ring-destructive/40",
        link: "text-primary underline-offset-4 hover:underline",
      },
      size: {
        default:
          "h-8 gap-1.5 px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
        xs: "h-6 gap-1 rounded-[min(var(--radius-md),10px)] px-2 text-xs in-data-[slot=button-group]:rounded-lg has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3",
        sm: "h-7 gap-1 rounded-[min(var(--radius-md),12px)] px-2.5 text-[0.8rem] in-data-[slot=button-group]:rounded-lg has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3.5",
        lg: "h-9 gap-1.5 px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
        icon: "size-8",
        "icon-xs":
          "size-6 rounded-[min(var(--radius-md),10px)] in-data-[slot=button-group]:rounded-lg [&_svg:not([class*='size-'])]:size-3",
        "icon-sm":
          "size-7 rounded-[min(var(--radius-md),12px)] in-data-[slot=button-group]:rounded-lg",
        "icon-lg": "size-9",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Button({
  className,
  variant = "default",
  size = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean
  }) {
  const Comp = asChild ? Slot.Root : "button"

  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button, buttonVariants }
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "components/ui/input.tsx".into(),
                content: r###"import * as React from "react"
import { cn } from "@/lib/utils"

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<"input">>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        type={type}
        className={cn(
          "flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
          className
        )}
        ref={ref}
        {...props}
      />
    )
  }
)
Input.displayName = "Input"

export { Input }
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "components/providers.tsx".into(),
                content: r###"'use client';

import React, { useEffect, useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ThemeProvider as NextThemesProvider } from 'next-themes';
import { useAppStore } from '@/store/useAppStore';

export function Providers({ children }: { children: React.ReactNode }) {
  // Rehydrate Zustand store on mount (client side only) to prevent SSR hydration errors
  useEffect(() => {
    useAppStore.persist.rehydrate();
  }, []);

  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            staleTime: 1000 * 60 * 5, // 5 minutes
            retry: 1,
            refetchOnWindowFocus: false,
          },
        },
      })
  );

  return (
    <QueryClientProvider client={queryClient}>
      <NextThemesProvider
        attribute="class"
        defaultTheme="system"
        enableSystem
        disableTransitionOnChange
      >
        {children}
      </NextThemesProvider>
    </QueryClientProvider>
  );
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "components/Navbar.tsx".into(),
                content: r###"'use client';

import React, { useState } from 'react';
import { useAuth } from '@/hooks/useAuth';
import { ThemeToggle } from './ThemeToggle';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { LogOut, User as UserIcon, Shield, Loader2, Home, Newspaper } from 'lucide-react';
import { LoginFormSchema, RegisterFormSchema } from '@/types/schema';

export function Navbar() {
  const { user, login, register, logout, isLoggingIn, isRegistering, isLoggingOut } = useAuth();
  const [isAuthModalOpen, setIsAuthModalOpen] = useState(false);
  const [authMode, setAuthMode] = useState<'login' | 'register'>('login');

  // Form states
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);

  const handleAuthSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    try {
      if (authMode === 'login') {
        const validated = LoginFormSchema.safeParse({ email, password });
        if (!validated.success) {
          setError(validated.error.issues[0].message);
          return;
        }
        await login({ email, password });
      } else {
        const validated = RegisterFormSchema.safeParse({ email, name, password });
        if (!validated.success) {
          setError(validated.error.issues[0].message);
          return;
        }
        await register({ email, name, password });
      }
      setIsAuthModalOpen(false);
      resetForm();
    } catch (err: any) {
      setError(err.response?.data?.error || 'Authentication failed. Please try again.');
    }
  };

  const resetForm = () => {
    setEmail('');
    setPassword('');
    setName('');
    setError(null);
  };

  const openAuth = (mode: 'login' | 'register') => {
    setAuthMode(mode);
    setError(null);
    setIsAuthModalOpen(true);
  };

  return (
    <>
      <nav className="fixed top-0 left-0 right-0 h-16 border-b border-border/40 bg-background/80 backdrop-blur-md z-40 flex items-center justify-between px-6 transition-colors duration-300">
        <div className="flex items-center gap-8">
          <a href="#" className="text-xl font-bold tracking-tight text-primary hover:opacity-80 transition-opacity">
            ONPKG <span className="text-xs bg-primary/10 text-primary px-2 py-0.5 rounded-full ml-1 font-normal">NEXT</span>
          </a>
          <div className="hidden md:flex items-center gap-6">
            <a href="#hero" className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors flex items-center gap-1.5">
              <Home className="w-4 h-4" /> Home
            </a>
            <a href="#posts" className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors flex items-center gap-1.5">
              <Newspaper className="w-4 h-4" /> Feed
            </a>
          </div>
        </div>

        <div className="flex items-center gap-4">
          <ThemeToggle />

          {user ? (
            <div className="flex items-center gap-4">
              <div className="hidden sm:flex flex-col items-end text-xs">
                <span className="font-semibold text-foreground flex items-center gap-1">
                  {user.role === 'ADMIN' && <Shield className="w-3.5 h-3.5 text-red-500" />}
                  {user.name || 'User'}
                </span>
                <span className="text-muted-foreground">{user.email}</span>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => logout()}
                disabled={isLoggingOut}
                className="flex items-center gap-2"
              >
                {isLoggingOut ? <Loader2 className="w-4 h-4 animate-spin" /> : <LogOut className="w-4 h-4" />}
                Logout
              </Button>
            </div>
          ) : (
            <div className="flex items-center gap-2">
              <Button variant="ghost" size="sm" onClick={() => openAuth('login')}>
                Sign In
              </Button>
              <Button size="sm" onClick={() => openAuth('register')}>
                Sign Up
              </Button>
            </div>
          )}
        </div>
      </nav>

      {/* Auth Modal overlay */}
      {isAuthModalOpen && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4 animate-in fade-in duration-200">
          <div className="bg-card text-card-foreground border border-border w-full max-w-md p-8 rounded-3xl shadow-2xl relative animate-in zoom-in-95 duration-200">
            <button
              onClick={() => { setIsAuthModalOpen(false); resetForm(); }}
              className="absolute top-4 right-4 text-muted-foreground hover:text-foreground text-xl font-semibold w-8 h-8 rounded-full flex items-center justify-center hover:bg-muted"
            >
              ×
            </button>
            <h3 className="text-2xl font-bold mb-2">
              {authMode === 'login' ? 'Welcome Back' : 'Create an Account'}
            </h3>
            <p className="text-muted-foreground text-sm mb-6">
              {authMode === 'login'
                ? 'Sign in to access database seeding, posts, and server actions.'
                : 'Sign up to create your profile and share posts in the feed.'
              }
            </p>

            <form onSubmit={handleAuthSubmit} className="space-y-4">
              {error && (
                <div className="p-3 bg-destructive/10 text-destructive text-sm rounded-xl border border-destructive/20 font-medium">
                  {error}
                </div>
              )}

              {authMode === 'register' && (
                <div className="space-y-1.5">
                  <label className="text-xs font-semibold text-muted-foreground">Full Name</label>
                  <Input
                    type="text"
                    placeholder="Aswin Dev"
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    required
                  />
                </div>
              )}

              <div className="space-y-1.5">
                <label className="text-xs font-semibold text-muted-foreground">Email Address</label>
                <Input
                  type="email"
                  placeholder="name@example.com"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  required
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-semibold text-muted-foreground">Password</label>
                <Input
                  type="password"
                  placeholder="••••••••"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                />
              </div>

              <Button type="submit" className="w-full h-11 font-semibold text-base mt-2" disabled={isLoggingIn || isRegistering}>
                {isLoggingIn || isRegistering ? (
                  <Loader2 className="w-5 h-5 animate-spin mr-2" />
                ) : null}
                {authMode === 'login' ? 'Sign In' : 'Sign Up'}
              </Button>
            </form>

            <div className="mt-6 text-center text-sm text-muted-foreground">
              {authMode === 'login' ? (
                <>
                  Don't have an account?{' '}
                  <button
                    onClick={() => openAuth('register')}
                    className="text-primary font-semibold hover:underline"
                  >
                    Sign Up
                  </button>
                </>
              ) : (
                <>
                  Already have an account?{' '}
                  <button
                    onClick={() => openAuth('login')}
                    className="text-primary font-semibold hover:underline"
                  >
                    Sign In
                  </button>
                </>
              )}
            </div>
          </div>
        </div>
      )}
    </>
  );
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "components/ThemeToggle.tsx".into(),
                content: r###"'use client';

import { useTheme } from 'next-themes';
import { Sun, Moon } from 'lucide-react';
import { Button } from './ui/button';
import { useEffect, useState } from 'react';

export function ThemeToggle() {
  const { setTheme, resolvedTheme } = useTheme();
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setMounted(true);
  }, []);

  if (!mounted) {
    return <div className="w-10 h-10 rounded-md bg-muted/40" />;
  }

  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={() => setTheme(resolvedTheme === 'dark' ? 'light' : 'dark')}
      className="w-10 h-10 rounded-full hover:bg-accent hover:text-accent-foreground"
      title="Toggle Theme"
    >
      {resolvedTheme === 'dark' ? (
        <Sun className="h-5 w-5 text-yellow-400 transition-all" />
      ) : (
        <Moon className="h-5 w-5 text-indigo-600 transition-all" />
      )}
      <span className="sr-only">Toggle theme</span>
    </Button>
  );
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "eslint.config.mjs".into(),
                content: r###"import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTs from "eslint-config-next/typescript";

const eslintConfig = defineConfig([
  ...nextVitals,
  ...nextTs,
  // Override default ignores of eslint-config-next.
  globalIgnores([
    // Default ignores of eslint-config-next:
    ".next/**",
    "out/**",
    "build/**",
    "next-env.d.ts",
  ]),
]);

export default eslintConfig;
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "app/globals.css".into(),
                content: r###"@import "tailwindcss";
@import "tw-animate-css";
@import "shadcn/tailwind.css";

@custom-variant dark (&:is(.dark *));

@theme inline {
  --color-background: var(--background);
  --color-foreground: var(--foreground);
  --font-sans: var(--font-sans);
  --font-mono: var(--font-geist-mono);
  --font-heading: var(--font-sans);
  --color-sidebar-ring: var(--sidebar-ring);
  --color-sidebar-border: var(--sidebar-border);
  --color-sidebar-accent-foreground: var(--sidebar-accent-foreground);
  --color-sidebar-accent: var(--sidebar-accent);
  --color-sidebar-primary-foreground: var(--sidebar-primary-foreground);
  --color-sidebar-primary: var(--sidebar-primary);
  --color-sidebar-foreground: var(--sidebar-foreground);
  --color-sidebar: var(--sidebar);
  --color-chart-5: var(--chart-5);
  --color-chart-4: var(--chart-4);
  --color-chart-3: var(--chart-3);
  --color-chart-2: var(--chart-2);
  --color-chart-1: var(--chart-1);
  --color-ring: var(--ring);
  --color-input: var(--input);
  --color-border: var(--border);
  --color-destructive: var(--destructive);
  --color-accent-foreground: var(--accent-foreground);
  --color-accent: var(--accent);
  --color-muted-foreground: var(--muted-foreground);
  --color-muted: var(--muted);
  --color-secondary-foreground: var(--secondary-foreground);
  --color-secondary: var(--secondary);
  --color-primary-foreground: var(--primary-foreground);
  --color-primary: var(--primary);
  --color-popover-foreground: var(--popover-foreground);
  --color-popover: var(--popover);
  --color-card-foreground: var(--card-foreground);
  --color-card: var(--card);
  --radius-sm: calc(var(--radius) * 0.6);
  --radius-md: calc(var(--radius) * 0.8);
  --radius-lg: var(--radius);
  --radius-xl: calc(var(--radius) * 1.4);
  --radius-2xl: calc(var(--radius) * 1.8);
  --radius-3xl: calc(var(--radius) * 2.2);
  --radius-4xl: calc(var(--radius) * 2.6);
}

:root {
  --background: oklch(1 0 0);
  --foreground: oklch(0.145 0 0);
  --card: oklch(1 0 0);
  --card-foreground: oklch(0.145 0 0);
  --popover: oklch(1 0 0);
  --popover-foreground: oklch(0.145 0 0);
  --primary: oklch(0.205 0 0);
  --primary-foreground: oklch(0.985 0 0);
  --secondary: oklch(0.97 0 0);
  --secondary-foreground: oklch(0.205 0 0);
  --muted: oklch(0.97 0 0);
  --muted-foreground: oklch(0.556 0 0);
  --accent: oklch(0.97 0 0);
  --accent-foreground: oklch(0.205 0 0);
  --destructive: oklch(0.577 0.245 27.325);
  --border: oklch(0.922 0 0);
  --input: oklch(0.922 0 0);
  --ring: oklch(0.708 0 0);
  --chart-1: oklch(0.87 0 0);
  --chart-2: oklch(0.556 0 0);
  --chart-3: oklch(0.439 0 0);
  --chart-4: oklch(0.371 0 0);
  --chart-5: oklch(0.269 0 0);
  --radius: 0.625rem;
  --sidebar: oklch(0.985 0 0);
  --sidebar-foreground: oklch(0.145 0 0);
  --sidebar-primary: oklch(0.205 0 0);
  --sidebar-primary-foreground: oklch(0.985 0 0);
  --sidebar-accent: oklch(0.97 0 0);
  --sidebar-accent-foreground: oklch(0.205 0 0);
  --sidebar-border: oklch(0.922 0 0);
  --sidebar-ring: oklch(0.708 0 0);
}

.dark {
  --background: oklch(0.145 0 0);
  --foreground: oklch(0.985 0 0);
  --card: oklch(0.205 0 0);
  --card-foreground: oklch(0.985 0 0);
  --popover: oklch(0.205 0 0);
  --popover-foreground: oklch(0.985 0 0);
  --primary: oklch(0.922 0 0);
  --primary-foreground: oklch(0.205 0 0);
  --secondary: oklch(0.269 0 0);
  --secondary-foreground: oklch(0.985 0 0);
  --muted: oklch(0.269 0 0);
  --muted-foreground: oklch(0.708 0 0);
  --accent: oklch(0.269 0 0);
  --accent-foreground: oklch(0.985 0 0);
  --destructive: oklch(0.704 0.191 22.216);
  --border: oklch(1 0 0 / 10%);
  --input: oklch(1 0 0 / 15%);
  --ring: oklch(0.556 0 0);
  --chart-1: oklch(0.87 0 0);
  --chart-2: oklch(0.556 0 0);
  --chart-3: oklch(0.439 0 0);
  --chart-4: oklch(0.371 0 0);
  --chart-5: oklch(0.269 0 0);
  --sidebar: oklch(0.205 0 0);
  --sidebar-foreground: oklch(0.985 0 0);
  --sidebar-primary: oklch(0.488 0.243 264.376);
  --sidebar-primary-foreground: oklch(0.985 0 0);
  --sidebar-accent: oklch(0.269 0 0);
  --sidebar-accent-foreground: oklch(0.985 0 0);
  --sidebar-border: oklch(1 0 0 / 10%);
  --sidebar-ring: oklch(0.556 0 0);
}

@layer base {
  * {
    @apply border-border outline-ring/50;
  }
  body {
    @apply bg-background text-foreground;
  }
  button:not(:disabled), [role="button"]:not(:disabled) {
    cursor: pointer;
  }
  html {
    @apply font-sans;
  }
}"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "app/layout.tsx".into(),
                content: r###"import type { Metadata } from "next";
import { Geist, Geist_Mono, Inter } from "next/font/google";
import "./globals.css";
import { cn } from "@/lib/utils";
import { Providers } from "@/components/providers";

const inter = Inter({ subsets: ['latin'], variable: '--font-sans' });

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "Onpkg Next.js Template",
  description: "Modern full-stack starter template with Next.js, Bun, Tailwind CSS v4, Shadcn UI, and Prisma",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      suppressHydrationWarning
      className={cn(
        "h-full",
        "antialiased",
        geistSans.variable,
        geistMono.variable,
        "font-sans",
        inter.variable
      )}
    >
      <body className="min-h-full flex flex-col bg-background text-foreground transition-colors duration-300">
        <Providers>
          {children}
        </Providers>
      </body>
    </html>
  );
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "app/page.tsx".into(),
                content: r###"'use client';

import React, { useState } from 'react';
import { useAuth } from '@/hooks/useAuth';
import { usePosts } from '@/hooks/usePosts';
import { Navbar } from '@/components/Navbar';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from '@/components/ui/card';
import {
  Zap,
  ShieldCheck,
  Package,
  Database,
  Flame,
  ArrowRight,
  Loader2,
  Lock,
  PlusCircle,
  RefreshCw,
  Sparkles,
  BookOpen
} from 'lucide-react';
import apiClient from '@/lib/api-client';
import { useQueryClient } from '@tanstack/react-query';

export default function Home() {
  const { user } = useAuth();
  const { posts, isLoading: postsLoading, createPost, isCreating } = usePosts();
  const queryClient = useQueryClient();

  const [postTitle, setPostTitle] = useState('');
  const [postContent, setPostContent] = useState('');
  const [postError, setPostError] = useState<string | null>(null);

  const [seeding, setSeeding] = useState(false);
  const [seedStatus, setSeedStatus] = useState<string | null>(null);

  const handleCreatePost = async (e: React.FormEvent) => {
    e.preventDefault();
    setPostError(null);

    const token = typeof window !== 'undefined' ? localStorage.getItem('auth-token') : null;

    if (!token) {
      setPostError('You must be signed in to post.');
      return;
    }

    try {
      await createPost({
        title: postTitle,
        content: postContent,
        published: true,
        token
      });
      setPostTitle('');
      setPostContent('');
    } catch (err: any) {
      setPostError(err.response?.data?.error || 'Failed to create post.');
    }
  };

  const handleSeedDatabase = async () => {
    setSeeding(true);
    setSeedStatus(null);
    try {
      const { data } = await apiClient.post<{ success: boolean; message: string }>('/api/db/seed');
      if (data.success) {
        setSeedStatus('Database seeded successfully!');
        queryClient.invalidateQueries({ queryKey: ['posts'] });
      }
    } catch (err: any) {
      setSeedStatus(err.response?.data?.error || 'Seeding failed.');
    } finally {
      setSeeding(false);
    }
  };

  return (
    <div className="min-h-screen flex flex-col bg-background text-foreground transition-colors duration-300">
      <Navbar />

      <main className="flex-1 pt-16">
        {/* Hero Section */}
        <section id="hero" className="relative min-h-[70vh] flex flex-col items-center justify-center p-8 text-center overflow-hidden border-b border-border/40">
          <div className="absolute -inset-10 bg-gradient-to-tr from-primary/10 via-transparent to-accent/15 blur-3xl opacity-60 rounded-full" />
          <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[500px] h-[500px] bg-primary/5 rounded-full blur-3xl" />

          <div className="relative z-10 max-w-3xl space-y-6">
            <div className="inline-flex items-center gap-2 px-3 py-1.5 rounded-full bg-primary/10 text-primary text-xs font-semibold tracking-wide border border-primary/20 animate-pulse">
              <Sparkles className="w-3.5 h-3.5" /> Upgraded Fullstack Template
            </div>

            <h1 className="text-5xl md:text-7xl font-bold tracking-tight leading-none bg-gradient-to-r from-foreground via-foreground/90 to-muted-foreground bg-clip-text text-transparent">
              Next.js 16 + Bun <br />
              <span className="bg-gradient-to-r from-primary to-purple-600 bg-clip-text text-transparent">Prisma 7 & Tailwind v4</span>
            </h1>

            <p className="text-muted-foreground text-lg md:text-xl max-w-2xl mx-auto leading-relaxed">
              The ultimate high-performance production template. Blazing fast, client-hydrate safe,
              type-safe validations, secure JWT auth, and relational database integrations.
            </p>

            <div className="flex flex-wrap items-center justify-center gap-4 pt-4">
              <a href="#feed">
                <Button size="lg" className="h-12 px-6 rounded-xl font-medium shadow-md shadow-primary/25 hover:shadow-lg transition-all flex items-center gap-2 group">
                  Explore Feed <ArrowRight className="w-4 h-4 group-hover:translate-x-1 transition-transform" />
                </Button>
              </a>
              <a href="#features">
                <Button size="lg" variant="outline" className="h-12 px-6 rounded-xl font-medium border-border/60 hover:bg-accent/40">
                  Tech Stack
                </Button>
              </a>
            </div>
          </div>
        </section>

        {/* Features Grid */}
        <section id="features" className="py-20 px-6 max-w-6xl mx-auto space-y-12">
          <div className="text-center space-y-3">
            <h2 className="text-3xl font-bold tracking-tight">Core Architecture</h2>
            <p className="text-muted-foreground max-w-xl mx-auto">
              Pre-configured tools combined to give you the most efficient developer experience.
            </p>
          </div>

          <div className="grid sm:grid-cols-2 lg:grid-cols-4 gap-6">
            <Card className="hover:border-primary/40 transition-all duration-300">
              <CardHeader className="space-y-2">
                <div className="w-10 h-10 rounded-xl bg-orange-500/10 flex items-center justify-center">
                  <Flame className="w-5 h-5 text-orange-500" />
                </div>
                <CardTitle>Bun Runtime</CardTitle>
                <CardDescription>Lightning fast bundler and package manager for zero startup overhead.</CardDescription>
              </CardHeader>
            </Card>

            <Card className="hover:border-primary/40 transition-all duration-300">
              <CardHeader className="space-y-2">
                <div className="w-10 h-10 rounded-xl bg-blue-500/10 flex items-center justify-center">
                  <Zap className="w-5 h-5 text-blue-500" />
                </div>
                <CardTitle>Next.js 16</CardTitle>
                <CardDescription>Advanced App Router with React Canary & Async Route Params support.</CardDescription>
              </CardHeader>
            </Card>

            <Card className="hover:border-primary/40 transition-all duration-300">
              <CardHeader className="space-y-2">
                <div className="w-10 h-10 rounded-xl bg-purple-500/10 flex items-center justify-center">
                  <Database className="w-5 h-5 text-purple-500" />
                </div>
                <CardTitle>Prisma 7 Client</CardTitle>
                <CardDescription>Next-gen ORM with Rust-free native JS driver adapters and PostgreSQL.</CardDescription>
              </CardHeader>
            </Card>

            <Card className="hover:border-primary/40 transition-all duration-300">
              <CardHeader className="space-y-2">
                <div className="w-10 h-10 rounded-xl bg-emerald-500/10 flex items-center justify-center">
                  <ShieldCheck className="w-5 h-5 text-emerald-500" />
                </div>
                <CardTitle>Zod & Zustand</CardTitle>
                <CardDescription>Type-safe form schemas combined with hydrate-safe global Zustand state.</CardDescription>
              </CardHeader>
            </Card>
          </div>
        </section>

        {/* Database & Interactive Actions */}
        <section className="py-12 border-t border-b border-border/40 bg-muted/30">
          <div className="max-w-4xl mx-auto px-6 grid md:grid-cols-2 gap-8 items-center">
            <div className="space-y-4">
              <h3 className="text-2xl font-bold tracking-tight flex items-center gap-2">
                <Database className="w-6 h-6 text-primary" /> Dev Sandbox Tools
              </h3>
              <p className="text-muted-foreground leading-relaxed">
                Reset your database, wipe existing collections, and seed beautiful dummy user and post
                relations directly via this API wrapper trigger. Make testing HMR and queries instantly visual.
              </p>

              <div className="flex items-center gap-4">
                <Button
                  onClick={handleSeedDatabase}
                  disabled={seeding}
                  className="flex items-center gap-2 rounded-xl"
                >
                  {seeding ? <Loader2 className="w-4 h-4 animate-spin" /> : <RefreshCw className="w-4 h-4" />}
                  Seed Mock Database
                </Button>
                {seedStatus && (
                  <span className="text-sm font-medium text-primary animate-pulse">{seedStatus}</span>
                )}
              </div>
            </div>

            <Card className="bg-card/40 backdrop-blur-sm">
              <CardHeader>
                <CardTitle className="flex items-center gap-2 text-base">
                  <BookOpen className="w-4 h-4 text-purple-500" /> CLI Seeding Shortcut
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-2 text-xs font-mono bg-muted/60 p-4 rounded-xl mx-4 border text-muted-foreground">
                <div># Seeding directly via Bun CLI:</div>
                <div className="text-foreground">bun run db:seed</div>
                <div className="mt-4"># Spin up Prisma Studio to view database:</div>
                <div className="text-foreground">bun run db:studio</div>
              </CardContent>
            </Card>
          </div>
        </section>

        {/* Feed & Post Form Section */}
        <section id="feed" className="py-20 max-w-5xl mx-auto px-6 grid md:grid-cols-3 gap-8">

          {/* Post Form Panel (Left) */}
          <div className="md:col-span-1 space-y-6">
            <h3 className="text-xl font-bold tracking-tight">Post Panel</h3>
            {user ? (
              <Card className="border-primary/20 bg-card/50">
                <CardHeader>
                  <CardTitle className="text-base flex items-center gap-2">
                    <PlusCircle className="w-4 h-4 text-primary" /> Create a Post
                  </CardTitle>
                  <CardDescription>Share your thoughts on fullstack development.</CardDescription>
                </CardHeader>
                <form onSubmit={handleCreatePost}>
                  <CardContent className="space-y-4">
                    {postError && (
                      <div className="p-3 bg-destructive/10 text-destructive text-xs rounded-lg font-medium border border-destructive/20">
                        {postError}
                      </div>
                    )}
                    <div className="space-y-1.5">
                      <label className="text-xs font-semibold text-muted-foreground">Title</label>
                      <Input
                        type="text"
                        placeholder="Next.js 16 is amazing!"
                        value={postTitle}
                        onChange={(e) => setPostTitle(e.target.value)}
                        required
                      />
                    </div>
                    <div className="space-y-1.5">
                      <label className="text-xs font-semibold text-muted-foreground">Content</label>
                      <textarea
                        className="flex min-h-[100px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                        placeholder="Write your post here..."
                        value={postContent}
                        onChange={(e) => setPostContent(e.target.value)}
                        required
                      />
                    </div>
                  </CardContent>
                  <CardFooter className="bg-transparent border-none">
                    <Button type="submit" className="w-full" disabled={isCreating}>
                      {isCreating ? <Loader2 className="w-4 h-4 animate-spin mr-2" /> : null}
                      Post to Feed
                    </Button>
                  </CardFooter>
                </form>
              </Card>
            ) : (
              <Card className="border-dashed bg-muted/20">
                <CardHeader className="text-center p-6 space-y-4">
                  <div className="mx-auto w-12 h-12 rounded-full bg-muted flex items-center justify-center">
                    <Lock className="w-5 h-5 text-muted-foreground" />
                  </div>
                  <div className="space-y-1">
                    <CardTitle className="text-base">Authenticated Only</CardTitle>
                    <CardDescription className="text-xs">
                      Sign in or create an account to post thoughts directly to the database.
                    </CardDescription>
                  </div>
                </CardHeader>
              </Card>
            )}
          </div>

          {/* Feed List Panel (Right) */}
          <div className="md:col-span-2 space-y-6">
            <div className="flex items-center justify-between">
              <h3 className="text-xl font-bold tracking-tight">Active Feed</h3>
              <span className="text-xs px-2 py-0.5 bg-muted text-muted-foreground rounded-full border">
                {posts.length} {posts.length === 1 ? 'post' : 'posts'}
              </span>
            </div>

            {postsLoading ? (
              <div className="flex flex-col items-center justify-center py-20 space-y-3">
                <Loader2 className="w-8 h-8 animate-spin text-primary" />
                <p className="text-muted-foreground text-sm">Querying database posts...</p>
              </div>
            ) : posts.length === 0 ? (
              <Card className="bg-muted/10 border-dashed py-20 text-center">
                <CardContent className="space-y-2">
                  <p className="text-muted-foreground">No posts found in database.</p>
                  <p className="text-xs text-muted-foreground/80">Click the 'Seed Mock Database' button above to populate.</p>
                </CardContent>
              </Card>
            ) : (
              <div className="space-y-4">
                {posts.map((post) => (
                  <Card key={post.id} className="hover:shadow-md transition-shadow">
                    <CardHeader className="pb-2">
                      <div className="flex justify-between items-start">
                        <CardTitle className="text-lg font-semibold leading-snug">{post.title}</CardTitle>
                        <span className="text-[10px] bg-secondary/80 text-secondary-foreground font-semibold px-2 py-0.5 rounded-full border border-secondary">
                          {post.views} views
                        </span>
                      </div>
                      <CardDescription className="text-xs">
                        Posted by <span className="font-semibold text-foreground">{post.author?.name || 'Anonymous'}</span> ({post.author?.email})
                      </CardDescription>
                    </CardHeader>
                    <CardContent>
                      <p className="text-muted-foreground leading-relaxed whitespace-pre-line text-sm">
                        {post.content}
                      </p>
                    </CardContent>
                    <CardFooter className="py-2 text-[10px] text-muted-foreground flex justify-between bg-muted/20 border-t">
                      <span>ID: {post.id}</span>
                      <span>{new Date(post.createdAt).toLocaleDateString()}</span>
                    </CardFooter>
                  </Card>
                ))}
              </div>
            )}
          </div>
        </section>
      </main>

      <footer className="py-8 border-t border-border/40 bg-muted/10 text-center text-xs text-muted-foreground">
        <p>© 2026 Onpkg. Next.js 16 + Prisma 7 Template Starter.</p>
      </footer>
    </div>
  );
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "app/api/db/seed/route.ts".into(),
                content: r###"import { NextRequest, NextResponse } from 'next/server';
import { prisma } from '@/lib/prisma';
import bcrypt from 'bcryptjs';
import { logger } from '@/lib/logger';
import { withLogging } from '@/lib/api-logger';

async function seedHandler(request: NextRequest) {
  if (process.env.NODE_ENV === 'production') {
    return NextResponse.json(
      { success: false, error: 'Seeding API disabled in production' },
      { status: 403 }
    );
  }

  try {
    logger.info('Database seeding requested via API...');

    await prisma.post.deleteMany();
    await prisma.user.deleteMany();

    const adminPassword = await bcrypt.hash('admin123', 10);
    const userPassword = await bcrypt.hash('user123', 10);

    const admin = await prisma.user.create({
      data: {
        email: 'admin@onpkg.com',
        name: 'Admin User',
        password: adminPassword,
        role: 'ADMIN',
      },
    });

    const user1 = await prisma.user.create({
      data: {
        email: 'user1@onpkg.com',
        name: 'Aswin Dev',
        password: userPassword,
        role: 'USER',
      },
    });

    const user2 = await prisma.user.create({
      data: {
        email: 'user2@onpkg.com',
        name: 'Jane Smith',
        password: userPassword,
        role: 'USER',
      },
    });

    await prisma.post.createMany({
      data: [
        {
          title: 'Getting Started with Next.js 16 and Prisma 7',
          content: 'Next.js 16 and Prisma 7 provide an incredibly powerful combo for full-stack React applications. By combining Next.js Server Actions with Prisma driver adapters, you can build blazing fast, edge-ready applications.',
          published: true,
          authorId: user1.id,
          views: 125,
        },
        {
          title: 'Building Beautiful Interfaces with Tailwind CSS v4',
          content: 'Tailwind CSS v4 introduces a streamlined engine, CSS-first configuration, and native cascading layers. It makes managing design systems a breeze without the bloat of traditional CSS setups.',
          published: true,
          authorId: user1.id,
          views: 348,
        },
        {
          title: 'The Future of State Management with Zustand',
          content: 'Zustand is a small, fast, and scalable bear-bones state-management solution. It has a comfy API based on hooks, is not opinionated, and doesn\'t wrap your app in providers.',
          published: true,
          authorId: user2.id,
          views: 99,
        },
        {
          title: 'Next.js 16 Asynchronous Route Parameters',
          content: 'In Next.js 16, page and route parameters (params) are now resolved as Promises. You must await params before reading their values to ensure compatibility and runtime speed.',
          published: true,
          authorId: admin.id,
          views: 42,
        },
      ],
    });

    logger.info('Database seeded successfully via API!');
    return NextResponse.json({ success: true, message: 'Database seeded successfully' });
  } catch (error: any) {
    logger.error('Error seeding database via API:', error);
    return NextResponse.json(
      { success: false, error: error.message || 'Failed to seed database' },
      { status: 500 }
    );
  }
}

export const POST = withLogging(seedHandler);
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "app/api/posts/route.ts".into(),
                content: r###"import { NextRequest, NextResponse } from 'next/server';
import { prisma } from '@/lib/prisma';
import { CreatePostSchema } from '@/types/schema';
import { verifyToken } from '@/lib/jwt';
import { logger } from '@/lib/logger';
import { withLogging } from '@/lib/api-logger';

async function getHandler(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url);
    const limit = parseInt(searchParams.get('limit') || '10', 10);

    const posts = await prisma.post.findMany({
      take: limit,
      orderBy: { createdAt: 'desc' },
      include: {
        author: {
          select: {
            id: true,
            name: true,
            email: true,
            role: true,
          },
        },
      },
    });

    return NextResponse.json({ success: true, data: posts });
  } catch (error: any) {
    logger.error('Error fetching posts in API:', error);
    return NextResponse.json(
      { success: false, error: 'Failed to fetch posts' },
      { status: 500 }
    );
  }
}

async function postHandler(request: NextRequest) {
  try {
    const authHeader = request.headers.get('Authorization');
    let userId: string | null = null;

    if (authHeader && authHeader.startsWith('Bearer ')) {
      const token = authHeader.split(' ')[1];
      const payload = verifyToken(token);
      if (payload) {
        userId = payload.userId;
      }
    }

    if (!userId) {
      return NextResponse.json(
        { success: false, error: 'Unauthorized' },
        { status: 401 }
      );
    }

    const body = await request.json();
    const validatedData = CreatePostSchema.safeParse(body);

    if (!validatedData.success) {
      return NextResponse.json(
        { success: false, errors: validatedData.error.flatten().fieldErrors },
        { status: 400 }
      );
    }

    const post = await prisma.post.create({
      data: {
        title: validatedData.data.title,
        content: validatedData.data.content,
        published: validatedData.data.published,
        authorId: userId,
      },
      include: {
        author: {
          select: {
            id: true,
            name: true,
            email: true,
          },
        },
      },
    });

    return NextResponse.json({ success: true, data: post }, { status: 201 });
  } catch (error: any) {
    logger.error('Error creating post in API:', error);
    return NextResponse.json(
      { success: false, error: 'Internal Server Error' },
      { status: 500 }
    );
  }
}

export const GET = withLogging(getHandler);
export const POST = withLogging(postHandler);
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "app/api/auth/register/route.ts".into(),
                content: r###"import { NextRequest, NextResponse } from 'next/server';
import { prisma } from '@/lib/prisma';
import { RegisterFormSchema } from '@/types/schema';
import { hashPassword } from '@/lib/auth-utils';
import { signToken } from '@/lib/jwt';
import { logger } from '@/lib/logger';
import { withLogging } from '@/lib/api-logger';

async function registerHandler(request: NextRequest) {
  try {
    const body = await request.json();
    const validatedData = RegisterFormSchema.safeParse(body);

    if (!validatedData.success) {
      return NextResponse.json(
        { success: false, errors: validatedData.error.flatten().fieldErrors },
        { status: 400 }
      );
    }

    const { email, name, password } = validatedData.data;

    const existingUser = await prisma.user.findUnique({
      where: { email },
    });

    if (existingUser) {
      return NextResponse.json(
        { success: false, error: 'User with this email already exists' },
        { status: 409 }
      );
    }

    const hashedPassword = await hashPassword(password);

    const user = await prisma.user.create({
      data: {
        email,
        name,
        password: hashedPassword,
        role: 'USER',
      },
    });

    const token = signToken({
      userId: user.id,
      email: user.email,
      role: user.role,
    });

    const responseUser = {
      id: user.id,
      email: user.email,
      name: user.name,
      role: user.role,
    };

    const response = NextResponse.json(
      { success: true, data: responseUser, token },
      { status: 201 }
    );

    response.cookies.set('token', token, {
      httpOnly: true,
      secure: process.env.NODE_ENV === 'production',
      sameSite: 'strict',
      maxAge: 60 * 60 * 24 * 7,
      path: '/',
    });

    return response;
  } catch (error: any) {
    logger.error('Registration Error:', error);
    return NextResponse.json(
      { success: false, error: 'Internal Server Error' },
      { status: 500 }
    );
  }
}

export const POST = withLogging(registerHandler);
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "app/api/auth/me/route.ts".into(),
                content: r###"import { NextRequest, NextResponse } from 'next/server';
import { prisma } from '@/lib/prisma';
import { verifyToken } from '@/lib/jwt';
import { logger } from '@/lib/logger';
import { withLogging } from '@/lib/api-logger';

async function getHandler(request: NextRequest) {
  try {
    let token = request.cookies.get('token')?.value;

    if (!token) {
      const authHeader = request.headers.get('Authorization');
      if (authHeader && authHeader.startsWith('Bearer ')) {
        token = authHeader.split(' ')[1];
      }
    }

    if (!token) {
      return NextResponse.json(
        { success: false, error: 'Unauthorized' },
        { status: 401 }
      );
    }

    const payload = verifyToken(token);

    if (!payload) {
      return NextResponse.json(
        { success: false, error: 'Unauthorized or expired token' },
        { status: 401 }
      );
    }

    const user = await prisma.user.findUnique({
      where: { id: payload.userId },
      select: {
        id: true,
        email: true,
        name: true,
        role: true,
        createdAt: true,
      },
    });

    if (!user) {
      return NextResponse.json(
        { success: false, error: 'User not found' },
        { status: 404 }
      );
    }

    return NextResponse.json({ success: true, data: user });
  } catch (error: any) {
    logger.error('Auth check error:', error);
    return NextResponse.json(
      { success: false, error: 'Internal Server Error' },
      { status: 500 }
    );
  }
}

async function postHandler() {
  const response = NextResponse.json({ success: true, message: 'Logged out successfully' });
  response.cookies.set('token', '', {
    httpOnly: true,
    expires: new Date(0),
    path: '/',
  });
  return response;
}

export const GET = withLogging(getHandler);
export const POST = withLogging(postHandler);
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "app/api/auth/login/route.ts".into(),
                content: r###"import { NextRequest, NextResponse } from 'next/server';
import { prisma } from '@/lib/prisma';
import { LoginFormSchema } from '@/types/schema';
import { comparePassword } from '@/lib/auth-utils';
import { signToken } from '@/lib/jwt';
import { logger } from '@/lib/logger';
import { withLogging } from '@/lib/api-logger';

async function loginHandler(request: NextRequest) {
  try {
    const body = await request.json();
    const validatedData = LoginFormSchema.safeParse(body);

    if (!validatedData.success) {
      return NextResponse.json(
        { success: false, errors: validatedData.error.flatten().fieldErrors },
        { status: 400 }
      );
    }

    const { email, password } = validatedData.data;

    const user = await prisma.user.findUnique({
      where: { email },
    });

    if (!user) {
      return NextResponse.json(
        { success: false, error: 'Invalid email or password' },
        { status: 401 }
      );
    }

    const isValidPassword = await comparePassword(password, user.password);

    if (!isValidPassword) {
      return NextResponse.json(
        { success: false, error: 'Invalid email or password' },
        { status: 401 }
      );
    }

    const token = signToken({
      userId: user.id,
      email: user.email,
      role: user.role,
    });

    const responseUser = {
      id: user.id,
      email: user.email,
      name: user.name,
      role: user.role,
    };

    const response = NextResponse.json(
      { success: true, data: responseUser, token },
      { status: 200 }
    );

    response.cookies.set('token', token, {
      httpOnly: true,
      secure: process.env.NODE_ENV === 'production',
      sameSite: 'strict',
      maxAge: 60 * 60 * 24 * 7,
      path: '/',
    });

    return response;
  } catch (error: any) {
    logger.error('Login Error:', error);
    return NextResponse.json(
      { success: false, error: 'Internal Server Error' },
      { status: 500 }
    );
  }
}

export const POST = withLogging(loginHandler);
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "app/favicon.ico".into(),
                content: "".into(),
                binary_content: Some(vec![0, 0, 1, 0, 4, 0, 16, 16, 0, 0, 1, 0, 32, 0, 40, 5, 0, 0, 70, 0, 0, 0, 32, 32, 0, 0, 1, 0, 32, 0, 40, 20, 0, 0, 110, 5, 0, 0, 48, 48, 0, 0, 1, 0, 32, 0, 40, 45, 0, 0, 150, 25, 0, 0, 0, 0, 0, 0, 1, 0, 32, 0, 141, 30, 0, 0, 190, 70, 0, 0, 40, 0, 0, 0, 16, 0, 0, 0, 32, 0, 0, 0, 1, 0, 32, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 36, 0, 0, 0, 93, 0, 0, 0, 186, 0, 0, 0, 186, 0, 0, 0, 93, 0, 0, 0, 36, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 160, 0, 0, 0, 242, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 242, 0, 0, 0, 160, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 56, 0, 0, 0, 224, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 224, 0, 0, 0, 56, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 31, 0, 0, 0, 226, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 226, 0, 0, 0, 31, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 161, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 161, 0, 0, 0, 0, 0, 0, 0, 35, 0, 0, 0, 244, 0, 0, 0, 255, 0, 0, 0, 255, 79, 79, 79, 255, 174, 174, 174, 255, 171, 171, 171, 255, 171, 171, 171, 255, 171, 171, 171, 255, 171, 171, 171, 255, 173, 173, 173, 255, 103, 103, 103, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 244, 0, 0, 0, 35, 0, 0, 0, 89, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 31, 31, 31, 255, 237, 237, 237, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 253, 253, 255, 53, 53, 53, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 89, 0, 0, 0, 187, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 107, 107, 107, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 142, 142, 142, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 187, 0, 0, 0, 187, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9, 255, 205, 205, 205, 255, 255, 255, 255, 255, 255, 255, 255, 255, 228, 228, 228, 255, 24, 24, 24, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 187, 0, 0, 0, 89, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 74, 74, 74, 255, 253, 253, 253, 255, 255, 255, 255, 255, 107, 107, 107, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 89, 0, 0, 0, 35, 0, 0, 0, 244, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 1, 1, 1, 255, 182, 182, 182, 255, 213, 213, 213, 255, 9, 9, 9, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 244, 0, 0, 0, 35, 0, 0, 0, 0, 0, 0, 0, 161, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 49, 49, 49, 255, 68, 68, 68, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 161, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 31, 0, 0, 0, 226, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 226, 0, 0, 0, 31, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 56, 0, 0, 0, 224, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 224, 0, 0, 0, 56, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 160, 0, 0, 0, 242, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 242, 0, 0, 0, 160, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 36, 0, 0, 0, 93, 0, 0, 0, 186, 0, 0, 0, 186, 0, 0, 0, 93, 0, 0, 0, 36, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 40, 0, 0, 0, 32, 0, 0, 0, 64, 0, 0, 0, 1, 0, 32, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 44, 0, 0, 0, 85, 0, 0, 0, 129, 0, 0, 0, 232, 0, 0, 0, 232, 0, 0, 0, 129, 0, 0, 0, 85, 0, 0, 0, 44, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 133, 0, 0, 0, 210, 0, 0, 0, 249, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 249, 0, 0, 0, 210, 0, 0, 0, 133, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 150, 0, 0, 0, 243, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 243, 0, 0, 0, 150, 0, 0, 0, 28, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 81, 0, 0, 0, 225, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 225, 0, 0, 0, 81, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 114, 0, 0, 0, 251, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 251, 0, 0, 0, 114, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 114, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 114, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 79, 0, 0, 0, 251, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 251, 0, 0, 0, 79, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 27, 0, 0, 0, 228, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 227, 0, 0, 0, 27, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 151, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 151, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 40, 0, 0, 0, 245, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 244, 0, 0, 0, 39, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 134, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 56, 56, 56, 255, 139, 139, 139, 255, 136, 136, 136, 255, 136, 136, 136, 255, 136, 136, 136, 255, 136, 136, 136, 255, 136, 136, 136, 255, 136, 136, 136, 255, 136, 136, 136, 255, 136, 136, 136, 255, 136, 136, 136, 255, 136, 136, 136, 255, 136, 136, 136, 255, 136, 136, 136, 255, 137, 137, 137, 255, 95, 95, 95, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 134, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 212, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 30, 30, 30, 255, 238, 238, 238, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 83, 83, 83, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 212, 0, 0, 0, 7, 0, 0, 0, 43, 0, 0, 0, 250, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 104, 104, 104, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 174, 174, 174, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 250, 0, 0, 0, 43, 0, 0, 0, 84, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 8, 8, 8, 255, 203, 203, 203, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 244, 244, 244, 255, 44, 44, 44, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 84, 0, 0, 0, 130, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 71, 71, 71, 255, 253, 253, 253, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 141, 141, 141, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 129, 0, 0, 0, 233, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 173, 173, 173, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 228, 228, 228, 255, 23, 23, 23, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 233, 0, 0, 0, 233, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 43, 43, 43, 255, 243, 243, 243, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 106, 106, 106, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 233, 0, 0, 0, 129, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 139, 139, 139, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 204, 204, 204, 255, 8, 8, 8, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 130, 0, 0, 0, 84, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 22, 22, 22, 255, 227, 227, 227, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 253, 253, 255, 73, 73, 73, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 84, 0, 0, 0, 43, 0, 0, 0, 250, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 104, 104, 104, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 175, 175, 175, 255, 1, 1, 1, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 250, 0, 0, 0, 43, 0, 0, 0, 7, 0, 0, 0, 212, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 8, 8, 8, 255, 203, 203, 203, 255, 255, 255, 255, 255, 255, 255, 255, 255, 244, 244, 244, 255, 44, 44, 44, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 212, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 134, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 71, 71, 71, 255, 253, 253, 253, 255, 255, 255, 255, 255, 141, 141, 141, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 134, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 39, 0, 0, 0, 244, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 177, 177, 177, 255, 236, 236, 236, 255, 23, 23, 23, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 245, 0, 0, 0, 40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 151, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 51, 51, 51, 255, 95, 95, 95, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 151, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 0, 0, 0, 227, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 228, 0, 0, 0, 27, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 79, 0, 0, 0, 251, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 251, 0, 0, 0, 79, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 114, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 114, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 114, 0, 0, 0, 251, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 251, 0, 0, 0, 114, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 81, 0, 0, 0, 225, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 225, 0, 0, 0, 81, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 150, 0, 0, 0, 243, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 243, 0, 0, 0, 150, 0, 0, 0, 28, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 133, 0, 0, 0, 210, 0, 0, 0, 249, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 249, 0, 0, 0, 210, 0, 0, 0, 133, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 44, 0, 0, 0, 85, 0, 0, 0, 129, 0, 0, 0, 232, 0, 0, 0, 232, 0, 0, 0, 129, 0, 0, 0, 85, 0, 0, 0, 44, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 40, 0, 0, 0, 48, 0, 0, 0, 96, 0, 0, 0, 1, 0, 32, 0, 0, 0, 0, 0, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 40, 0, 0, 0, 76, 0, 0, 0, 106, 0, 0, 0, 179, 0, 0, 0, 248, 0, 0, 0, 247, 0, 0, 0, 179, 0, 0, 0, 106, 0, 0, 0, 75, 0, 0, 0, 40, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 19, 0, 0, 0, 86, 0, 0, 0, 160, 0, 0, 0, 216, 0, 0, 0, 248, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 248, 0, 0, 0, 216, 0, 0, 0, 160, 0, 0, 0, 85, 0, 0, 0, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 37, 0, 0, 0, 139, 0, 0, 0, 225, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 225, 0, 0, 0, 139, 0, 0, 0, 38, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 22, 0, 0, 0, 139, 0, 0, 0, 239, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 239, 0, 0, 0, 139, 0, 0, 0, 22, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 81, 0, 0, 0, 220, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 220, 0, 0, 0, 82, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 138, 0, 0, 0, 254, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 254, 0, 0, 0, 138, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15, 0, 0, 0, 173, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 173, 0, 0, 0, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15, 0, 0, 0, 184, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 184, 0, 0, 0, 14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 174, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 174, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 138, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 138, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 80, 0, 0, 0, 253, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 253, 0, 0, 0, 79, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 21, 0, 0, 0, 223, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 223, 0, 0, 0, 21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 139, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 139, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 35, 0, 0, 0, 241, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 241, 0, 0, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 140, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 139, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 228, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 36, 36, 36, 255, 104, 104, 104, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 101, 101, 101, 255, 80, 80, 80, 255, 1, 1, 1, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 228, 0, 0, 0, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 85, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 28, 28, 28, 255, 235, 235, 235, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 115, 115, 115, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 85, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 161, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 101, 101, 101, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 204, 204, 204, 255, 8, 8, 8, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 161, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 218, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 7, 7, 7, 255, 201, 201, 201, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 253, 253, 255, 72, 72, 72, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 218, 0, 0, 0, 9, 0, 0, 0, 40, 0, 0, 0, 249, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 69, 69, 69, 255, 252, 252, 252, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 174, 174, 174, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 248, 0, 0, 0, 40, 0, 0, 0, 75, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 170, 170, 170, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 244, 244, 244, 255, 44, 44, 44, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 76, 0, 0, 0, 106, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 41, 41, 41, 255, 242, 242, 242, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 140, 140, 140, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 106, 0, 0, 0, 180, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 136, 136, 136, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 227, 227, 227, 255, 22, 22, 22, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 179, 0, 0, 0, 248, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 20, 20, 20, 255, 225, 225, 225, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 105, 105, 105, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 248, 0, 0, 0, 248, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 101, 101, 101, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 203, 203, 203, 255, 8, 8, 8, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 248, 0, 0, 0, 179, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 7, 7, 7, 255, 201, 201, 201, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 253, 253, 255, 72, 72, 72, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 180, 0, 0, 0, 106, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 69, 69, 69, 255, 252, 252, 252, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 174, 174, 174, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 106, 0, 0, 0, 76, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 170, 170, 170, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 244, 244, 244, 255, 44, 44, 44, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 75, 0, 0, 0, 40, 0, 0, 0, 248, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 41, 41, 41, 255, 242, 242, 242, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 140, 140, 140, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 249, 0, 0, 0, 40, 0, 0, 0, 9, 0, 0, 0, 218, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 136, 136, 136, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 227, 227, 227, 255, 22, 22, 22, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 218, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 0, 161, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 20, 20, 20, 255, 225, 225, 225, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 105, 105, 105, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 161, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 85, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 101, 101, 101, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 204, 204, 204, 255, 8, 8, 8, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 85, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 228, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 7, 7, 7, 255, 201, 201, 201, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 253, 253, 255, 72, 72, 72, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 228, 0, 0, 0, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 139, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 69, 69, 69, 255, 252, 252, 252, 255, 255, 255, 255, 255, 174, 174, 174, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 140, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 35, 0, 0, 0, 241, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 172, 172, 172, 255, 251, 251, 251, 255, 44, 44, 44, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 241, 0, 0, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 139, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 50, 50, 50, 255, 125, 125, 125, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 139, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 21, 0, 0, 0, 223, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 223, 0, 0, 0, 21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 79, 0, 0, 0, 253, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 253, 0, 0, 0, 80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 138, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 138, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 174, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 174, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 14, 0, 0, 0, 184, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 184, 0, 0, 0, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15, 0, 0, 0, 173, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 173, 0, 0, 0, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 138, 0, 0, 0, 254, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 254, 0, 0, 0, 138, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 82, 0, 0, 0, 220, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 220, 0, 0, 0, 81, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 22, 0, 0, 0, 139, 0, 0, 0, 239, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 239, 0, 0, 0, 139, 0, 0, 0, 22, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 38, 0, 0, 0, 139, 0, 0, 0, 225, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 225, 0, 0, 0, 139, 0, 0, 0, 37, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 19, 0, 0, 0, 85, 0, 0, 0, 160, 0, 0, 0, 216, 0, 0, 0, 248, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 248, 0, 0, 0, 216, 0, 0, 0, 160, 0, 0, 0, 86, 0, 0, 0, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 40, 0, 0, 0, 75, 0, 0, 0, 106, 0, 0, 0, 179, 0, 0, 0, 247, 0, 0, 0, 248, 0, 0, 0, 179, 0, 0, 0, 106, 0, 0, 0, 76, 0, 0, 0, 40, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 1, 0, 0, 0, 1, 0, 8, 6, 0, 0, 0, 92, 114, 168, 102, 0, 0, 0, 1, 115, 82, 71, 66, 0, 174, 206, 28, 233, 0, 0, 0, 56, 101, 88, 73, 102, 77, 77, 0, 42, 0, 0, 0, 8, 0, 1, 135, 105, 0, 4, 0, 0, 0, 1, 0, 0, 0, 26, 0, 0, 0, 0, 0, 2, 160, 2, 0, 4, 0, 0, 0, 1, 0, 0, 1, 0, 160, 3, 0, 4, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 68, 34, 56, 115, 0, 0, 30, 3, 73, 68, 65, 84, 120, 1, 237, 93, 9, 176, 21, 213, 153, 110, 148, 93, 8, 60, 22, 17, 81, 86, 65, 150, 232, 196, 104, 36, 9, 202, 78, 140, 142, 49, 51, 42, 136, 113, 176, 194, 100, 170, 6, 196, 141, 169, 73, 161, 136, 8, 152, 68, 147, 76, 50, 147, 170, 40, 206, 20, 40, 212, 152, 50, 214, 196, 153, 209, 24, 25, 71, 9, 139, 193, 113, 95, 64, 229, 177, 136, 224, 134, 138, 27, 18, 120, 8, 200, 155, 239, 187, 208, 143, 190, 143, 123, 111, 247, 189, 189, 157, 211, 253, 253, 85, 223, 123, 125, 187, 79, 159, 229, 59, 231, 255, 251, 244, 249, 207, 57, 221, 194, 145, 100, 129, 129, 174, 40, 68, 103, 160, 19, 208, 17, 56, 9, 232, 3, 244, 4, 78, 0, 186, 1, 93, 128, 238, 64, 11, 160, 14, 104, 3, 120, 165, 17, 63, 118, 0, 13, 192, 78, 224, 51, 224, 61, 96, 59, 240, 54, 240, 46, 240, 38, 176, 11, 224, 117, 226, 19, 224, 32, 32, 177, 148, 1, 54, 6, 137, 29, 12, 80, 129, 169, 208, 189, 128, 193, 64, 255, 195, 160, 82, 211, 0, 80, 249, 105, 4, 90, 3, 113, 202, 94, 68, 78, 227, 224, 26, 128, 119, 112, 188, 13, 216, 12, 188, 1, 208, 88, 208, 104, 208, 152, 72, 12, 103, 64, 6, 192, 204, 10, 58, 25, 217, 226, 19, 124, 8, 240, 53, 96, 32, 112, 34, 64, 3, 192, 39, 188, 201, 178, 7, 153, 115, 123, 13, 235, 112, 252, 42, 240, 50, 192, 222, 3, 141, 133, 196, 32, 6, 100, 0, 210, 175, 140, 150, 200, 2, 21, 124, 40, 48, 18, 248, 10, 48, 0, 224, 147, 62, 75, 242, 22, 10, 179, 5, 120, 30, 88, 3, 172, 7, 54, 1, 7, 0, 73, 74, 12, 200, 0, 164, 67, 124, 63, 36, 75, 69, 31, 7, 12, 7, 78, 1, 216, 125, 207, 147, 124, 138, 194, 242, 181, 225, 25, 224, 127, 1, 246, 22, 104, 32, 36, 9, 50, 32, 3, 144, 12, 217, 124, 47, 167, 194, 143, 0, 46, 0, 78, 3, 122, 0, 146, 35, 12, 188, 143, 195, 87, 128, 101, 192, 10, 224, 53, 128, 227, 13, 146, 24, 25, 144, 1, 136, 143, 220, 182, 136, 250, 12, 224, 59, 192, 121, 0, 149, 190, 21, 32, 241, 103, 224, 115, 4, 169, 7, 30, 6, 104, 16, 94, 0, 26, 0, 73, 196, 12, 200, 0, 68, 76, 40, 162, 59, 19, 184, 4, 56, 31, 160, 210, 31, 11, 72, 106, 103, 128, 99, 4, 28, 72, 124, 8, 120, 16, 160, 49, 160, 203, 82, 34, 6, 140, 97, 128, 239, 244, 211, 129, 149, 0, 71, 193, 217, 64, 133, 232, 57, 96, 47, 96, 37, 48, 3, 232, 13, 72, 196, 64, 106, 12, 240, 189, 126, 44, 176, 8, 160, 219, 75, 10, 159, 44, 7, 116, 41, 46, 6, 206, 1, 232, 73, 145, 136, 129, 68, 24, 232, 129, 84, 166, 2, 79, 2, 251, 1, 41, 126, 186, 28, 176, 14, 86, 3, 236, 129, 117, 5, 36, 98, 32, 22, 6, 232, 170, 155, 13, 208, 85, 37, 165, 55, 147, 131, 141, 168, 155, 249, 0, 95, 201, 36, 98, 32, 18, 6, 56, 65, 231, 215, 192, 7, 128, 20, 223, 14, 14, 248, 74, 246, 51, 96, 24, 32, 17, 3, 53, 49, 48, 4, 119, 81, 241, 63, 6, 164, 248, 118, 114, 240, 209, 225, 58, 252, 50, 254, 75, 196, 64, 32, 6, 164, 248, 118, 42, 123, 37, 35, 237, 26, 130, 65, 129, 90, 128, 2, 229, 146, 129, 94, 40, 245, 207, 1, 61, 241, 179, 103, 0, 92, 227, 192, 87, 131, 219, 1, 141, 17, 128, 4, 201, 33, 6, 58, 224, 223, 44, 96, 27, 224, 54, 20, 253, 207, 54, 23, 52, 4, 51, 129, 58, 64, 146, 99, 6, 46, 70, 217, 159, 3, 164, 240, 249, 228, 128, 117, 63, 17, 208, 60, 2, 144, 144, 39, 225, 20, 221, 7, 128, 131, 128, 148, 95, 28, 220, 143, 118, 112, 58, 32, 201, 56, 3, 29, 81, 190, 155, 1, 14, 10, 73, 241, 197, 129, 183, 13, 112, 123, 51, 206, 243, 208, 107, 1, 72, 200, 162, 140, 71, 161, 158, 2, 188, 149, 174, 99, 241, 209, 188, 13, 60, 141, 54, 50, 38, 139, 10, 144, 215, 50, 117, 67, 193, 255, 5, 224, 218, 242, 230, 149, 173, 223, 226, 164, 84, 27, 248, 51, 218, 10, 189, 5, 234, 13, 128, 4, 155, 133, 59, 238, 188, 8, 148, 170, 100, 157, 19, 47, 126, 109, 128, 189, 1, 46, 248, 146, 88, 198, 64, 123, 228, 119, 46, 176, 27, 240, 171, 100, 93, 23, 71, 149, 218, 0, 123, 3, 108, 75, 220, 224, 69, 98, 1, 3, 156, 255, 253, 40, 80, 169, 82, 117, 77, 252, 84, 219, 6, 30, 67, 155, 210, 218, 2, 195, 13, 192, 100, 228, 239, 93, 41, 191, 140, 95, 76, 109, 224, 61, 196, 251, 61, 64, 98, 24, 3, 156, 205, 247, 75, 128, 219, 71, 85, 107, 217, 21, 94, 156, 85, 211, 6, 216, 198, 216, 214, 248, 154, 41, 49, 128, 129, 1, 200, 3, 187, 103, 213, 84, 162, 194, 138, 175, 176, 109, 96, 57, 218, 92, 127, 3, 218, 127, 174, 179, 192, 81, 254, 205, 64, 216, 202, 212, 253, 226, 176, 150, 54, 240, 58, 218, 158, 213, 94, 2, 155, 119, 172, 157, 6, 242, 239, 1, 184, 69, 151, 68, 12, 164, 193, 0, 231, 9, 252, 53, 176, 19, 224, 186, 2, 235, 196, 70, 3, 208, 6, 44, 223, 6, 220, 10, 240, 88, 34, 6, 210, 100, 128, 109, 240, 60, 128, 110, 66, 126, 242, 236, 11, 64, 18, 19, 3, 180, 184, 247, 2, 181, 116, 215, 116, 143, 120, 139, 187, 13, 252, 59, 218, 102, 222, 62, 241, 22, 147, 170, 31, 29, 237, 73, 56, 181, 2, 136, 187, 18, 21, 191, 56, 14, 211, 6, 184, 67, 49, 219, 170, 36, 66, 6, 184, 149, 19, 167, 101, 134, 169, 24, 221, 43, 254, 146, 106, 3, 108, 171, 220, 69, 90, 18, 1, 3, 195, 17, 199, 38, 32, 169, 202, 83, 58, 226, 58, 138, 54, 176, 1, 109, 246, 235, 17, 180, 255, 92, 71, 49, 26, 165, 231, 23, 96, 162, 168, 16, 197, 33, 30, 147, 110, 3, 108, 187, 108, 195, 198, 138, 201, 94, 128, 81, 96, 237, 183, 64, 79, 99, 217, 83, 198, 196, 64, 101, 6, 184, 1, 205, 183, 1, 186, 8, 185, 231, 164, 113, 98, 170, 1, 112, 149, 255, 4, 227, 24, 83, 134, 196, 64, 117, 12, 112, 154, 186, 177, 70, 192, 68, 3, 64, 229, 191, 15, 208, 147, 191, 186, 134, 166, 208, 230, 50, 64, 35, 192, 185, 2, 198, 245, 4, 76, 51, 0, 163, 65, 146, 148, 31, 36, 72, 50, 199, 0, 95, 7, 104, 4, 158, 7, 182, 2, 70, 72, 11, 35, 114, 113, 40, 19, 28, 237, 255, 79, 224, 68, 131, 242, 164, 172, 136, 129, 168, 25, 224, 114, 117, 110, 71, 79, 87, 97, 234, 98, 138, 1, 24, 0, 38, 150, 1, 3, 83, 103, 68, 25, 16, 3, 241, 51, 192, 69, 68, 231, 3, 116, 111, 167, 42, 199, 164, 154, 250, 161, 196, 57, 107, 138, 221, 126, 41, 191, 1, 149, 161, 44, 36, 194, 0, 31, 120, 191, 1, 82, 159, 49, 152, 118, 15, 224, 75, 32, 225, 15, 192, 57, 128, 68, 12, 228, 141, 129, 85, 40, 240, 119, 1, 174, 38, 76, 69, 210, 28, 4, 228, 234, 169, 187, 128, 11, 83, 41, 185, 18, 21, 3, 233, 51, 208, 23, 89, 56, 25, 120, 4, 56, 0, 36, 46, 105, 26, 128, 5, 40, 237, 140, 196, 75, 172, 4, 197, 128, 89, 12, 156, 142, 236, 240, 97, 200, 93, 173, 18, 151, 180, 12, 192, 52, 148, 244, 199, 128, 9, 99, 16, 137, 147, 174, 4, 197, 64, 51, 6, 184, 102, 224, 3, 128, 243, 4, 18, 149, 52, 198, 0, 198, 162, 132, 255, 13, 208, 47, 42, 17, 3, 98, 224, 16, 3, 252, 254, 0, 199, 3, 254, 152, 36, 33, 73, 27, 128, 126, 135, 11, 216, 55, 201, 66, 42, 45, 49, 96, 9, 3, 111, 32, 159, 19, 0, 186, 9, 19, 145, 36, 187, 224, 199, 161, 68, 139, 129, 190, 137, 148, 76, 137, 136, 1, 251, 24, 224, 3, 242, 223, 0, 234, 74, 34, 146, 228, 24, 192, 79, 81, 162, 203, 19, 41, 149, 18, 17, 3, 246, 50, 64, 35, 208, 14, 120, 52, 137, 34, 36, 101, 0, 168, 248, 183, 3, 73, 246, 56, 146, 224, 79, 105, 136, 129, 56, 24, 248, 26, 34, 221, 0, 188, 26, 71, 228, 222, 56, 91, 120, 127, 196, 116, 60, 20, 241, 46, 7, 180, 180, 55, 38, 130, 21, 109, 38, 25, 216, 142, 82, 141, 7, 94, 139, 179, 116, 113, 247, 0, 216, 149, 225, 46, 190, 167, 197, 89, 8, 197, 45, 6, 50, 200, 0, 189, 100, 167, 2, 15, 0, 251, 227, 42, 95, 220, 6, 224, 38, 100, 124, 106, 92, 153, 87, 188, 98, 32, 227, 12, 244, 71, 249, 246, 1, 171, 226, 42, 103, 156, 175, 0, 163, 145, 233, 135, 129, 196, 70, 52, 227, 34, 73, 241, 138, 129, 20, 25, 216, 131, 180, 47, 0, 98, 49, 2, 113, 25, 128, 78, 200, 240, 227, 192, 89, 128, 68, 12, 136, 129, 112, 12, 188, 140, 219, 57, 129, 238, 227, 112, 209, 28, 125, 119, 92, 175, 0, 243, 145, 212, 164, 163, 147, 211, 25, 49, 32, 6, 106, 96, 128, 3, 232, 28, 79, 251, 159, 26, 238, 173, 120, 75, 28, 61, 128, 209, 72, 241, 247, 0, 247, 65, 147, 136, 1, 49, 16, 13, 3, 159, 35, 26, 190, 10, 68, 58, 85, 56, 106, 3, 192, 245, 253, 116, 249, 169, 235, 15, 18, 36, 98, 32, 98, 6, 158, 65, 124, 124, 21, 216, 29, 85, 188, 81, 191, 2, 252, 3, 50, 54, 37, 170, 204, 41, 30, 49, 32, 6, 138, 24, 232, 133, 95, 13, 192, 234, 162, 179, 33, 126, 68, 217, 3, 224, 186, 230, 149, 0, 191, 224, 43, 17, 3, 98, 32, 30, 6, 62, 65, 180, 220, 58, 127, 93, 20, 209, 31, 19, 69, 36, 136, 163, 53, 48, 23, 144, 242, 71, 68, 104, 82, 209, 116, 232, 208, 193, 233, 221, 187, 119, 82, 201, 41, 157, 240, 12, 80, 199, 22, 0, 145, 232, 110, 36, 145, 32, 51, 220, 214, 235, 18, 64, 98, 25, 3, 83, 167, 78, 117, 150, 44, 89, 226, 180, 106, 213, 202, 178, 156, 231, 58, 187, 23, 161, 244, 220, 59, 192, 8, 225, 192, 223, 243, 64, 163, 96, 23, 7, 199, 31, 127, 124, 227, 150, 45, 91, 26, 41, 147, 39, 79, 86, 253, 217, 213, 134, 169, 115, 70, 120, 218, 174, 147, 226, 219, 165, 248, 110, 125, 205, 155, 55, 175, 160, 252, 252, 179, 126, 253, 250, 198, 186, 186, 58, 25, 1, 187, 140, 192, 15, 81, 151, 169, 74, 95, 164, 206, 85, 75, 106, 56, 150, 113, 48, 120, 240, 224, 198, 29, 59, 118, 52, 25, 0, 30, 204, 158, 61, 91, 245, 104, 87, 61, 190, 9, 221, 235, 9, 164, 38, 183, 33, 101, 53, 26, 203, 56, 104, 209, 162, 69, 227, 221, 119, 223, 93, 164, 252, 252, 241, 225, 135, 31, 54, 246, 239, 223, 95, 245, 105, 87, 125, 254, 83, 90, 218, 63, 8, 9, 191, 47, 3, 96, 159, 1, 28, 59, 118, 108, 227, 190, 125, 251, 142, 50, 0, 60, 177, 104, 209, 34, 25, 0, 187, 12, 192, 167, 208, 65, 238, 185, 145, 184, 220, 129, 20, 213, 88, 44, 227, 160, 109, 219, 182, 141, 43, 86, 172, 40, 169, 252, 60, 185, 119, 239, 222, 198, 17, 35, 70, 168, 94, 237, 170, 215, 59, 147, 214, 254, 97, 72, 240, 99, 25, 0, 251, 12, 224, 148, 41, 83, 202, 42, 191, 123, 97, 249, 242, 229, 141, 112, 11, 202, 8, 216, 99, 4, 56, 57, 40, 209, 94, 128, 158, 254, 246, 52, 142, 38, 69, 166, 219, 111, 195, 134, 13, 174, 158, 87, 252, 47, 183, 160, 117, 198, 61, 177, 94, 128, 158, 254, 22, 42, 63, 123, 107, 94, 183, 95, 69, 237, 199, 197, 250, 250, 122, 185, 5, 237, 170, 103, 246, 2, 134, 0, 85, 73, 45, 139, 129, 232, 123, 28, 87, 85, 42, 10, 156, 58, 3, 112, 251, 57, 11, 23, 46, 116, 218, 183, 111, 31, 40, 47, 221, 186, 117, 115, 62, 251, 236, 51, 231, 137, 39, 158, 8, 20, 94, 129, 82, 103, 128, 223, 23, 228, 218, 158, 71, 226, 204, 73, 95, 68, 174, 145, 127, 187, 158, 12, 141, 229, 220, 126, 126, 189, 0, 185, 5, 173, 123, 13, 248, 8, 250, 217, 63, 78, 3, 48, 31, 145, 55, 189, 83, 234, 216, 14, 46, 198, 141, 27, 215, 184, 127, 255, 126, 63, 125, 47, 121, 93, 110, 65, 59, 234, 216, 163, 139, 179, 227, 50, 0, 93, 16, 241, 70, 79, 66, 50, 4, 22, 24, 67, 63, 183, 95, 73, 173, 247, 156, 148, 91, 208, 58, 3, 176, 5, 58, 218, 61, 14, 35, 192, 79, 122, 75, 233, 45, 227, 32, 136, 219, 207, 163, 239, 37, 15, 229, 22, 180, 174, 221, 127, 63, 106, 3, 208, 6, 17, 254, 73, 6, 192, 174, 134, 80, 141, 219, 175, 164, 230, 123, 78, 202, 45, 104, 85, 221, 255, 31, 116, 149, 58, 27, 153, 156, 131, 152, 248, 129, 2, 245, 0, 44, 226, 160, 26, 183, 159, 71, 215, 75, 30, 202, 45, 104, 85, 219, 231, 6, 162, 99, 131, 104, 127, 80, 55, 32, 7, 255, 206, 12, 18, 161, 194, 152, 193, 64, 181, 110, 63, 191, 92, 211, 45, 184, 107, 215, 46, 185, 5, 253, 136, 50, 227, 58, 245, 250, 32, 240, 96, 20, 217, 225, 70, 132, 219, 1, 61, 253, 45, 225, 160, 86, 183, 95, 201, 71, 191, 231, 164, 220, 130, 86, 233, 0, 221, 245, 125, 162, 48, 0, 87, 73, 249, 173, 170, 248, 70, 186, 253, 202, 173, 246, 243, 232, 115, 77, 135, 114, 11, 90, 213, 22, 56, 112, 31, 74, 184, 81, 220, 42, 64, 79, 127, 75, 56, 8, 235, 246, 243, 179, 10, 114, 11, 90, 165, 11, 212, 221, 150, 97, 44, 0, 223, 251, 119, 203, 0, 216, 83, 233, 87, 94, 121, 165, 159, 14, 135, 190, 46, 183, 160, 53, 237, 97, 15, 116, 55, 212, 216, 221, 143, 164, 252, 214, 84, 118, 35, 221, 126, 27, 55, 110, 12, 173, 224, 65, 34, 184, 252, 242, 203, 213, 43, 180, 163, 87, 248, 19, 232, 112, 77, 194, 197, 5, 47, 1, 170, 104, 75, 56, 136, 210, 237, 231, 103, 4, 228, 22, 180, 70, 47, 94, 132, 14, 183, 171, 197, 2, 140, 192, 77, 123, 101, 0, 236, 168, 232, 82, 155, 124, 250, 41, 113, 216, 235, 55, 221, 116, 147, 30, 14, 230, 63, 28, 56, 39, 224, 27, 181, 24, 128, 31, 75, 249, 237, 80, 126, 186, 253, 238, 185, 231, 158, 176, 250, 92, 245, 253, 114, 11, 218, 209, 62, 160, 199, 183, 85, 107, 0, 216, 101, 80, 247, 223, 124, 235, 94, 120, 2, 199, 233, 246, 243, 179, 10, 139, 23, 47, 86, 47, 192, 252, 118, 242, 2, 244, 153, 175, 244, 129, 229, 12, 132, 228, 8, 162, 42, 215, 112, 14, 226, 118, 251, 249, 25, 0, 185, 5, 173, 208, 17, 190, 202, 159, 93, 74, 251, 203, 125, 27, 144, 243, 136, 107, 26, 56, 40, 149, 136, 206, 197, 199, 192, 196, 137, 19, 157, 209, 163, 71, 199, 151, 128, 79, 204, 109, 218, 180, 113, 22, 44, 88, 160, 111, 11, 250, 240, 148, 242, 101, 46, 12, 226, 152, 94, 32, 161, 81, 120, 28, 208, 211, 223, 112, 14, 146, 116, 251, 249, 245, 4, 228, 22, 52, 94, 95, 30, 131, 78, 151, 123, 224, 23, 25, 134, 190, 248, 197, 121, 196, 50, 0, 134, 115, 144, 164, 219, 207, 207, 0, 208, 45, 216, 185, 115, 103, 181, 25, 115, 219, 204, 7, 208, 105, 234, 118, 145, 148, 90, 13, 56, 10, 33, 126, 80, 20, 74, 63, 140, 99, 128, 171, 253, 238, 186, 235, 174, 192, 155, 124, 198, 93, 0, 174, 22, 220, 189, 123, 183, 179, 122, 245, 234, 184, 147, 82, 252, 181, 49, 112, 28, 110, 227, 212, 224, 122, 239, 237, 165, 186, 4, 19, 188, 1, 116, 108, 30, 3, 112, 251, 57, 179, 102, 205, 114, 186, 118, 237, 106, 84, 230, 174, 191, 254, 122, 7, 223, 22, 52, 42, 79, 202, 76, 17, 3, 227, 139, 126, 149, 248, 209, 26, 231, 158, 3, 212, 149, 51, 152, 131, 52, 221, 126, 126, 175, 2, 114, 11, 26, 173, 59, 212, 109, 234, 120, 89, 25, 140, 43, 252, 192, 128, 12, 128, 161, 28, 164, 237, 246, 243, 51, 0, 114, 11, 26, 173, 59, 212, 109, 234, 120, 147, 52, 127, 5, 224, 247, 197, 58, 55, 93, 213, 129, 113, 12, 76, 154, 52, 41, 85, 183, 159, 31, 33, 114, 11, 250, 49, 148, 234, 117, 234, 54, 191, 236, 213, 36, 205, 13, 192, 55, 155, 174, 232, 192, 56, 6, 186, 119, 239, 238, 204, 153, 51, 199, 184, 124, 53, 207, 16, 62, 63, 238, 92, 122, 233, 165, 205, 79, 235, 183, 25, 12, 156, 235, 205, 134, 215, 0, 240, 248, 44, 239, 69, 29, 155, 197, 192, 140, 25, 51, 156, 129, 3, 7, 154, 149, 169, 50, 185, 185, 229, 150, 91, 156, 186, 186, 186, 50, 87, 117, 58, 69, 6, 190, 130, 180, 155, 188, 127, 94, 3, 112, 2, 46, 12, 72, 49, 99, 74, 186, 2, 3, 116, 251, 93, 125, 245, 213, 21, 66, 152, 117, 233, 212, 83, 79, 117, 174, 186, 138, 187, 201, 73, 12, 99, 128, 79, 144, 158, 165, 242, 196, 37, 131, 7, 0, 13, 0, 26, 198, 65, 90, 171, 253, 252, 6, 252, 252, 174, 107, 181, 160, 145, 186, 68, 29, 63, 199, 53, 0, 222, 30, 64, 81, 215, 192, 13, 160, 255, 233, 51, 192, 119, 234, 43, 174, 184, 34, 253, 140, 84, 153, 3, 206, 83, 192, 158, 1, 85, 222, 165, 224, 49, 51, 192, 238, 63, 7, 251, 11, 226, 53, 0, 77, 39, 221, 139, 250, 159, 62, 3, 112, 251, 21, 6, 254, 90, 181, 106, 149, 126, 102, 106, 200, 1, 13, 215, 136, 17, 129, 215, 161, 212, 144, 130, 110, 169, 129, 129, 166, 177, 62, 215, 0, 240, 255, 105, 53, 68, 164, 91, 98, 102, 32, 237, 213, 126, 97, 139, 39, 183, 96, 88, 6, 99, 185, 159, 227, 0, 45, 188, 49, 243, 107, 162, 155, 1, 189, 255, 27, 196, 129, 73, 171, 253, 252, 222, 247, 253, 174, 107, 181, 160, 81, 186, 197, 175, 124, 243, 107, 223, 77, 203, 3, 233, 1, 40, 57, 50, 200, 64, 146, 116, 24, 176, 201, 237, 231, 199, 16, 221, 130, 88, 45, 232, 23, 76, 215, 147, 97, 128, 250, 78, 52, 25, 128, 147, 113, 220, 158, 39, 36, 102, 48, 64, 183, 31, 13, 64, 86, 132, 110, 65, 155, 220, 152, 89, 225, 189, 76, 57, 58, 226, 124, 47, 94, 115, 199, 0, 250, 151, 9, 168, 211, 41, 48, 96, 234, 106, 191, 176, 84, 92, 119, 221, 117, 90, 45, 24, 150, 196, 232, 238, 31, 194, 168, 92, 3, 160, 9, 64, 209, 17, 27, 58, 166, 177, 99, 237, 116, 251, 249, 21, 156, 123, 6, 200, 45, 232, 199, 82, 98, 215, 251, 49, 37, 215, 0, 244, 73, 44, 89, 37, 84, 145, 1, 219, 221, 126, 21, 11, 135, 139, 114, 11, 250, 49, 148, 216, 245, 194, 67, 159, 6, 128, 31, 15, 44, 188, 15, 36, 150, 180, 18, 42, 203, 128, 233, 171, 253, 202, 102, 60, 224, 5, 185, 5, 3, 18, 21, 127, 48, 122, 254, 142, 165, 1, 232, 4, 104, 213, 70, 252, 132, 251, 166, 0, 183, 159, 21, 171, 253, 124, 11, 226, 19, 128, 175, 56, 90, 45, 232, 67, 82, 252, 151, 185, 157, 84, 103, 215, 0, 208, 8, 72, 82, 102, 128, 139, 103, 108, 89, 237, 23, 150, 170, 185, 115, 231, 202, 45, 24, 150, 196, 112, 247, 211, 39, 43, 3, 16, 142, 195, 232, 238, 182, 109, 181, 95, 216, 146, 103, 205, 205, 25, 150, 143, 20, 238, 231, 67, 191, 19, 123, 0, 244, 9, 242, 195, 1, 146, 148, 24, 200, 170, 219, 207, 143, 206, 153, 51, 103, 202, 45, 232, 71, 82, 124, 215, 185, 55, 96, 71, 26, 128, 222, 241, 165, 161, 152, 131, 48, 144, 85, 183, 159, 95, 217, 181, 90, 208, 143, 161, 216, 175, 159, 68, 3, 160, 41, 192, 177, 243, 92, 62, 129, 172, 187, 253, 202, 151, 252, 208, 21, 185, 5, 253, 24, 138, 245, 122, 31, 26, 0, 78, 3, 150, 164, 196, 64, 214, 221, 126, 126, 180, 202, 45, 232, 199, 80, 172, 215, 123, 114, 115, 128, 191, 5, 180, 23, 64, 172, 60, 151, 142, 156, 155, 124, 46, 93, 186, 212, 184, 15, 124, 148, 206, 109, 124, 103, 251, 245, 235, 231, 172, 95, 191, 222, 121, 229, 149, 87, 226, 75, 68, 49, 151, 98, 224, 77, 189, 2, 148, 162, 37, 161, 115, 92, 28, 147, 23, 183, 159, 31, 165, 114, 11, 250, 49, 20, 203, 245, 110, 236, 1, 92, 11, 20, 150, 6, 198, 146, 132, 34, 45, 201, 0, 221, 96, 11, 23, 46, 52, 230, 219, 126, 37, 51, 153, 224, 73, 125, 91, 48, 65, 178, 143, 36, 181, 147, 61, 0, 45, 210, 62, 66, 72, 34, 71, 174, 219, 143, 141, 94, 114, 132, 1, 173, 22, 60, 194, 69, 66, 71, 221, 105, 0, 218, 37, 148, 152, 146, 57, 204, 64, 94, 221, 126, 126, 13, 64, 171, 5, 253, 24, 138, 252, 250, 49, 220, 23, 236, 11, 128, 134, 64, 146, 0, 3, 116, 251, 45, 91, 182, 204, 232, 207, 123, 37, 64, 67, 217, 36, 240, 109, 65, 103, 252, 248, 241, 206, 154, 53, 107, 202, 134, 209, 133, 200, 24, 248, 156, 138, 47, 229, 143, 140, 79, 255, 136, 108, 223, 228, 211, 191, 132, 225, 66, 208, 64, 46, 88, 176, 192, 177, 117, 23, 228, 112, 165, 79, 252, 238, 54, 28, 4, 156, 151, 120, 178, 57, 77, 80, 110, 191, 96, 21, 223, 183, 111, 95, 167, 190, 190, 94, 110, 193, 96, 116, 133, 10, 165, 167, 127, 40, 250, 170, 187, 153, 131, 92, 114, 251, 249, 115, 198, 65, 210, 121, 243, 230, 233, 219, 130, 254, 84, 133, 14, 161, 30, 64, 104, 10, 131, 69, 48, 108, 216, 48, 231, 206, 59, 239, 116, 218, 181, 211, 152, 107, 16, 198, 184, 78, 160, 161, 161, 193, 89, 181, 106, 85, 144, 224, 10, 83, 35, 3, 236, 1, 124, 94, 227, 189, 186, 45, 32, 3, 124, 162, 221, 112, 195, 13, 78, 151, 46, 133, 173, 216, 3, 222, 165, 96, 215, 94, 123, 173, 122, 76, 241, 54, 131, 131, 52, 0, 159, 196, 155, 134, 98, 231, 168, 246, 101, 151, 93, 38, 34, 170, 100, 128, 6, 115, 206, 156, 57, 85, 222, 165, 224, 85, 48, 240, 33, 13, 192, 193, 42, 110, 80, 208, 42, 25, 96, 151, 255, 230, 155, 111, 214, 168, 118, 149, 188, 185, 193, 105, 56, 71, 141, 26, 229, 254, 212, 255, 104, 25, 104, 160, 1, 216, 17, 109, 156, 138, 205, 203, 192, 228, 201, 147, 157, 115, 207, 61, 215, 123, 74, 199, 85, 48, 192, 213, 130, 243, 231, 207, 119, 90, 183, 230, 254, 21, 146, 136, 25, 248, 148, 131, 128, 236, 155, 246, 141, 56, 98, 69, 7, 6, 122, 244, 232, 225, 44, 89, 178, 68, 239, 254, 33, 91, 67, 159, 62, 125, 156, 141, 27, 55, 58, 107, 215, 174, 13, 25, 147, 110, 111, 198, 192, 102, 246, 0, 62, 106, 118, 82, 63, 35, 98, 128, 131, 88, 3, 6, 12, 136, 40, 182, 252, 70, 195, 65, 84, 190, 70, 213, 213, 105, 243, 234, 136, 91, 193, 123, 236, 1, 176, 127, 122, 118, 196, 17, 231, 62, 186, 161, 67, 135, 202, 237, 23, 97, 43, 144, 91, 48, 66, 50, 143, 68, 181, 156, 61, 128, 237, 71, 126, 235, 40, 10, 6, 248, 196, 186, 241, 198, 27, 213, 245, 143, 130, 76, 79, 28, 215, 92, 115, 141, 220, 130, 30, 62, 34, 56, 124, 139, 6, 96, 91, 4, 17, 41, 10, 15, 3, 19, 38, 76, 112, 56, 248, 39, 137, 150, 1, 246, 2, 228, 22, 140, 148, 211, 237, 52, 0, 111, 71, 26, 101, 206, 35, 163, 219, 143, 31, 192, 108, 217, 146, 95, 92, 147, 68, 205, 128, 220, 130, 145, 50, 90, 216, 18, 108, 23, 162, 220, 23, 105, 180, 57, 142, 140, 13, 116, 228, 200, 145, 57, 102, 32, 222, 162, 203, 45, 24, 25, 191, 156, 1, 188, 139, 61, 128, 157, 135, 17, 89, 204, 121, 141, 136, 110, 63, 117, 81, 227, 175, 125, 26, 88, 205, 172, 12, 205, 115, 65, 239, 105, 0, 62, 61, 140, 208, 49, 230, 61, 2, 14, 82, 201, 237, 23, 127, 43, 224, 32, 43, 13, 173, 220, 130, 161, 184, 46, 50, 0, 154, 11, 16, 138, 75, 236, 171, 14, 183, 223, 244, 233, 211, 67, 198, 162, 219, 131, 50, 48, 104, 208, 32, 135, 203, 171, 37, 53, 51, 192, 53, 64, 133, 77, 65, 185, 37, 152, 166, 3, 215, 204, 163, 227, 200, 237, 23, 130, 188, 16, 183, 106, 91, 245, 16, 228, 57, 206, 59, 184, 251, 0, 95, 1, 40, 175, 31, 250, 167, 191, 181, 48, 192, 213, 126, 114, 251, 213, 194, 92, 184, 123, 244, 109, 193, 80, 252, 21, 220, 255, 174, 1, 120, 35, 84, 84, 57, 190, 153, 110, 63, 190, 143, 202, 237, 151, 78, 35, 160, 225, 149, 215, 165, 38, 238, 11, 15, 125, 215, 0, 172, 175, 41, 10, 221, 84, 24, 141, 86, 3, 76, 175, 33, 184, 223, 22, 212, 106, 193, 170, 235, 96, 11, 239, 112, 13, 0, 223, 7, 56, 31, 64, 82, 5, 3, 114, 251, 85, 65, 86, 140, 65, 229, 22, 172, 154, 220, 61, 184, 227, 45, 222, 229, 26, 128, 247, 112, 76, 72, 170, 96, 64, 110, 191, 42, 200, 138, 49, 168, 220, 130, 85, 147, 203, 245, 63, 5, 125, 119, 13, 0, 93, 2, 236, 5, 72, 2, 50, 64, 183, 223, 180, 105, 211, 2, 134, 86, 176, 184, 25, 160, 91, 144, 203, 175, 37, 129, 24, 224, 244, 255, 130, 235, 223, 53, 0, 141, 56, 177, 41, 208, 173, 10, 84, 96, 128, 155, 124, 114, 20, 90, 98, 14, 3, 90, 45, 24, 184, 46, 214, 33, 100, 97, 43, 64, 215, 0, 240, 206, 231, 2, 223, 158, 243, 128, 90, 237, 103, 102, 3, 144, 91, 48, 112, 189, 188, 230, 134, 244, 26, 0, 158, 228, 164, 32, 73, 5, 6, 180, 201, 103, 5, 114, 12, 184, 36, 183, 160, 111, 37, 80, 199, 95, 114, 67, 121, 13, 192, 86, 156, 124, 223, 189, 160, 255, 165, 25, 224, 34, 20, 109, 242, 89, 154, 27, 19, 206, 202, 45, 232, 91, 11, 28, 0, 108, 218, 3, 196, 107, 0, 120, 65, 227, 0, 21, 248, 147, 219, 175, 2, 57, 6, 93, 162, 91, 112, 210, 164, 73, 6, 229, 200, 168, 172, 112, 2, 80, 147, 199, 207, 107, 0, 138, 186, 6, 70, 101, 217, 144, 204, 200, 237, 103, 72, 69, 248, 100, 131, 110, 65, 109, 34, 90, 150, 36, 142, 245, 53, 125, 11, 132, 155, 130, 122, 229, 56, 252, 144, 233, 244, 50, 114, 248, 88, 155, 124, 150, 32, 197, 224, 83, 28, 16, 220, 187, 119, 175, 179, 114, 229, 74, 131, 115, 153, 74, 214, 126, 129, 84, 155, 102, 254, 122, 123, 0, 204, 205, 171, 0, 247, 7, 144, 120, 24, 208, 106, 63, 15, 25, 22, 29, 106, 181, 224, 81, 149, 69, 221, 110, 242, 0, 240, 106, 115, 3, 192, 249, 193, 124, 71, 144, 120, 24, 208, 183, 253, 60, 100, 88, 116, 40, 183, 224, 81, 149, 69, 221, 46, 172, 1, 112, 175, 52, 55, 0, 220, 27, 240, 41, 247, 162, 254, 59, 133, 207, 121, 115, 181, 95, 171, 86, 173, 68, 135, 133, 12, 200, 45, 88, 84, 105, 212, 237, 162, 253, 63, 155, 27, 0, 134, 126, 188, 232, 150, 156, 255, 144, 219, 207, 238, 6, 32, 183, 96, 81, 253, 61, 86, 244, 11, 63, 154, 15, 2, 242, 250, 94, 224, 10, 128, 3, 130, 185, 22, 186, 253, 150, 46, 93, 170, 15, 124, 88, 222, 10, 244, 109, 193, 66, 5, 126, 128, 191, 63, 2, 138, 198, 248, 74, 245, 0, 222, 68, 160, 151, 11, 183, 228, 252, 143, 220, 126, 217, 104, 0, 114, 11, 22, 234, 145, 243, 255, 169, 219, 69, 82, 170, 7, 192, 133, 65, 221, 128, 243, 138, 66, 230, 236, 7, 221, 126, 119, 220, 113, 135, 211, 190, 125, 251, 156, 149, 60, 155, 197, 149, 91, 208, 185, 3, 53, 251, 100, 243, 218, 45, 213, 3, 96, 152, 53, 0, 63, 28, 144, 75, 113, 221, 126, 108, 52, 146, 236, 48, 64, 183, 224, 41, 167, 156, 146, 157, 2, 5, 47, 73, 3, 130, 174, 40, 21, 188, 156, 1, 224, 135, 216, 139, 252, 133, 165, 110, 206, 234, 57, 185, 253, 178, 89, 179, 52, 232, 156, 33, 152, 67, 217, 136, 50, 215, 151, 42, 119, 57, 3, 192, 129, 192, 71, 75, 221, 144, 245, 115, 238, 38, 159, 114, 251, 101, 179, 166, 115, 250, 233, 182, 63, 160, 54, 217, 11, 56, 74, 202, 25, 0, 6, 124, 8, 40, 242, 25, 30, 117, 119, 6, 79, 228, 180, 129, 100, 176, 38, 75, 23, 41, 135, 110, 65, 190, 202, 63, 82, 154, 141, 210, 110, 64, 55, 44, 183, 12, 186, 8, 56, 193, 61, 145, 245, 255, 114, 251, 101, 189, 134, 15, 149, 143, 110, 193, 77, 155, 54, 57, 107, 215, 242, 77, 55, 243, 194, 233, 253, 183, 2, 7, 74, 149, 180, 82, 15, 128, 93, 134, 101, 165, 110, 202, 234, 57, 185, 253, 178, 90, 179, 197, 229, 226, 32, 47, 63, 225, 158, 147, 111, 11, 62, 140, 210, 243, 149, 190, 164, 148, 114, 3, 122, 3, 114, 210, 192, 20, 32, 243, 243, 96, 229, 246, 243, 86, 123, 246, 143, 187, 117, 235, 230, 52, 52, 52, 56, 171, 86, 173, 202, 114, 97, 247, 160, 112, 179, 0, 238, 245, 81, 82, 90, 148, 60, 123, 228, 100, 75, 28, 46, 7, 70, 30, 57, 149, 189, 35, 126, 84, 226, 190, 251, 238, 115, 46, 190, 248, 226, 236, 21, 78, 37, 42, 203, 0, 13, 192, 240, 225, 195, 157, 117, 235, 56, 71, 38, 147, 178, 26, 165, 26, 15, 236, 47, 87, 58, 42, 120, 37, 225, 123, 195, 125, 64, 166, 13, 0, 187, 130, 108, 4, 245, 245, 245, 78, 99, 35, 231, 65, 73, 242, 192, 192, 177, 199, 30, 155, 245, 105, 222, 247, 163, 30, 203, 42, 63, 235, 216, 175, 7, 192, 48, 125, 128, 103, 128, 227, 249, 67, 34, 6, 196, 128, 21, 12, 112, 219, 175, 179, 128, 138, 223, 251, 168, 52, 8, 232, 150, 146, 27, 8, 210, 143, 40, 17, 3, 98, 192, 30, 6, 56, 128, 95, 81, 249, 89, 148, 32, 6, 128, 225, 238, 5, 114, 55, 39, 128, 5, 151, 136, 1, 11, 25, 96, 183, 255, 238, 32, 249, 14, 106, 0, 184, 54, 224, 133, 32, 17, 42, 140, 24, 16, 3, 169, 51, 192, 87, 246, 103, 131, 228, 34, 168, 1, 224, 108, 162, 127, 13, 18, 161, 194, 136, 1, 49, 144, 58, 3, 236, 177, 83, 103, 125, 37, 200, 32, 160, 27, 9, 7, 1, 185, 165, 80, 63, 247, 132, 254, 139, 1, 49, 96, 28, 3, 155, 144, 163, 175, 3, 31, 7, 201, 89, 208, 30, 0, 227, 226, 142, 34, 139, 130, 68, 170, 48, 98, 64, 12, 164, 198, 0, 221, 246, 129, 148, 159, 57, 172, 166, 7, 192, 240, 253, 1, 190, 91, 116, 225, 15, 137, 24, 16, 3, 70, 49, 192, 135, 244, 112, 96, 107, 208, 92, 85, 211, 3, 96, 156, 220, 82, 248, 183, 65, 35, 87, 56, 49, 32, 6, 18, 101, 96, 41, 82, 219, 90, 77, 138, 213, 246, 0, 24, 247, 16, 128, 91, 11, 117, 230, 15, 137, 24, 16, 3, 70, 48, 240, 9, 114, 113, 46, 192, 213, 127, 129, 165, 218, 30, 0, 35, 94, 15, 240, 61, 67, 34, 6, 196, 128, 57, 12, 80, 39, 171, 82, 126, 102, 189, 150, 30, 0, 239, 27, 10, 112, 110, 128, 122, 1, 100, 67, 34, 6, 210, 101, 160, 166, 167, 63, 179, 92, 75, 15, 128, 247, 189, 6, 168, 23, 64, 38, 36, 98, 32, 125, 6, 106, 122, 250, 51, 219, 181, 246, 0, 120, 47, 123, 1, 28, 11, 232, 196, 31, 18, 49, 32, 6, 82, 97, 128, 35, 255, 124, 247, 231, 198, 159, 85, 75, 173, 61, 0, 38, 196, 94, 128, 230, 5, 84, 77, 185, 110, 16, 3, 145, 50, 192, 57, 255, 53, 41, 63, 115, 17, 166, 7, 192, 251, 123, 2, 79, 3, 39, 243, 135, 68, 12, 136, 129, 68, 25, 224, 146, 223, 111, 0, 91, 107, 77, 213, 111, 75, 48, 191, 120, 255, 140, 0, 220, 84, 100, 130, 95, 64, 93, 23, 3, 98, 32, 114, 6, 230, 32, 198, 71, 195, 196, 26, 182, 7, 192, 180, 59, 0, 171, 128, 175, 242, 135, 68, 12, 136, 129, 68, 24, 224, 234, 220, 49, 192, 103, 97, 82, 11, 219, 3, 96, 218, 220, 39, 96, 7, 48, 17, 136, 194, 160, 32, 26, 137, 24, 16, 3, 62, 12, 92, 141, 235, 47, 249, 132, 241, 189, 28, 102, 16, 208, 27, 249, 131, 248, 241, 144, 247, 132, 142, 197, 128, 24, 136, 141, 129, 223, 33, 102, 110, 247, 29, 90, 162, 124, 98, 159, 134, 220, 240, 85, 160, 46, 116, 174, 20, 129, 24, 16, 3, 229, 24, 224, 164, 159, 209, 192, 218, 114, 1, 170, 57, 31, 197, 43, 128, 155, 30, 253, 145, 173, 128, 177, 238, 9, 253, 23, 3, 98, 32, 114, 6, 110, 69, 140, 15, 68, 21, 107, 148, 61, 0, 230, 233, 56, 224, 143, 192, 217, 252, 33, 17, 3, 98, 32, 82, 6, 158, 67, 108, 227, 128, 80, 3, 127, 222, 28, 69, 217, 3, 96, 188, 220, 140, 144, 75, 134, 39, 3, 126, 223, 28, 64, 16, 137, 24, 16, 3, 1, 25, 160, 203, 125, 42, 176, 33, 96, 248, 64, 193, 162, 54, 0, 76, 244, 13, 160, 59, 192, 141, 9, 36, 98, 64, 12, 68, 195, 192, 63, 35, 154, 200, 103, 222, 70, 253, 10, 224, 22, 181, 11, 14, 86, 2, 28, 24, 148, 136, 1, 49, 16, 142, 1, 118, 253, 199, 3, 59, 195, 69, 115, 244, 221, 81, 185, 1, 155, 199, 204, 61, 201, 174, 3, 246, 52, 191, 160, 223, 98, 64, 12, 84, 197, 192, 110, 132, 254, 33, 16, 185, 242, 51, 23, 113, 188, 2, 48, 94, 202, 86, 128, 227, 0, 99, 0, 137, 24, 16, 3, 181, 49, 240, 19, 220, 182, 180, 182, 91, 253, 239, 138, 235, 21, 192, 77, 185, 61, 14, 56, 73, 136, 221, 23, 137, 24, 16, 3, 213, 49, 64, 143, 218, 133, 64, 67, 117, 183, 5, 15, 29, 183, 1, 96, 78, 184, 111, 192, 227, 0, 87, 14, 74, 196, 128, 24, 8, 198, 0, 87, 250, 209, 229, 199, 101, 247, 177, 73, 156, 175, 0, 110, 166, 185, 78, 224, 109, 224, 175, 128, 184, 198, 28, 220, 180, 244, 95, 12, 100, 129, 129, 3, 40, 196, 52, 96, 69, 220, 133, 73, 194, 0, 176, 12, 175, 2, 156, 34, 204, 181, 203, 18, 49, 32, 6, 42, 51, 240, 43, 92, 254, 69, 229, 32, 209, 92, 77, 226, 21, 192, 205, 41, 103, 9, 114, 193, 144, 166, 10, 187, 140, 232, 191, 24, 56, 154, 1, 62, 245, 191, 3, 112, 244, 63, 118, 73, 210, 0, 176, 48, 3, 128, 199, 128, 126, 252, 33, 17, 3, 98, 160, 136, 129, 173, 248, 197, 7, 36, 39, 211, 37, 34, 73, 191, 147, 191, 142, 82, 253, 29, 192, 105, 141, 18, 49, 32, 6, 142, 48, 176, 11, 135, 63, 0, 18, 83, 126, 38, 157, 212, 24, 0, 211, 114, 133, 5, 228, 146, 198, 11, 128, 164, 123, 32, 110, 30, 244, 95, 12, 152, 196, 192, 23, 200, 204, 76, 224, 63, 146, 206, 84, 26, 6, 128, 101, 228, 212, 198, 142, 192, 55, 249, 67, 34, 6, 114, 206, 192, 79, 81, 126, 34, 113, 73, 203, 0, 176, 160, 79, 0, 28, 11, 56, 157, 63, 36, 98, 32, 167, 12, 220, 139, 114, 255, 35, 64, 215, 95, 226, 146, 118, 23, 188, 19, 74, 204, 153, 130, 163, 18, 47, 185, 18, 20, 3, 233, 51, 240, 39, 100, 225, 47, 129, 200, 214, 247, 87, 91, 164, 164, 7, 1, 155, 231, 111, 39, 78, 252, 13, 240, 108, 243, 11, 250, 45, 6, 50, 206, 0, 219, 252, 229, 64, 106, 202, 79, 126, 211, 238, 1, 48, 15, 148, 129, 192, 50, 96, 0, 127, 72, 196, 64, 198, 25, 216, 132, 242, 157, 15, 208, 43, 150, 170, 164, 221, 3, 112, 11, 79, 66, 174, 0, 222, 117, 79, 232, 191, 24, 200, 40, 3, 108, 227, 83, 128, 212, 149, 159, 252, 166, 57, 8, 200, 244, 189, 242, 14, 126, 240, 99, 7, 223, 6, 58, 120, 47, 232, 88, 12, 100, 132, 1, 46, 240, 249, 30, 176, 198, 148, 242, 152, 100, 0, 200, 201, 86, 224, 121, 64, 70, 0, 36, 72, 50, 197, 0, 149, 159, 239, 252, 43, 77, 42, 149, 105, 6, 128, 220, 108, 5, 56, 79, 64, 70, 0, 36, 72, 50, 193, 0, 149, 159, 27, 229, 174, 50, 173, 52, 38, 26, 0, 114, 180, 13, 80, 79, 192, 180, 214, 162, 252, 212, 194, 128, 251, 228, 55, 78, 249, 89, 24, 83, 13, 0, 243, 182, 21, 160, 17, 248, 22, 192, 89, 131, 18, 49, 96, 27, 3, 28, 240, 227, 59, 255, 74, 83, 51, 110, 178, 1, 32, 103, 91, 129, 39, 129, 145, 64, 87, 64, 34, 6, 108, 97, 96, 51, 50, 202, 110, 63, 39, 251, 72, 66, 50, 112, 10, 238, 127, 26, 104, 20, 196, 129, 5, 109, 128, 109, 117, 16, 32, 137, 144, 129, 147, 16, 215, 106, 64, 70, 64, 28, 152, 220, 6, 86, 160, 141, 178, 173, 90, 33, 166, 191, 2, 120, 73, 228, 148, 201, 255, 2, 72, 174, 22, 16, 121, 153, 209, 177, 41, 12, 252, 6, 25, 249, 62, 192, 15, 229, 90, 33, 54, 25, 0, 18, 186, 23, 224, 148, 97, 206, 96, 228, 254, 130, 166, 204, 100, 68, 86, 36, 57, 102, 128, 235, 249, 127, 9, 112, 77, 191, 54, 187, 73, 168, 33, 76, 71, 58, 92, 76, 100, 114, 119, 80, 121, 203, 126, 253, 80, 225, 167, 37, 212, 230, 149, 76, 51, 6, 198, 225, 55, 231, 84, 75, 209, 196, 65, 26, 109, 128, 109, 111, 124, 179, 54, 169, 159, 9, 51, 208, 31, 233, 45, 7, 210, 104, 0, 74, 51, 191, 188, 63, 142, 54, 55, 32, 225, 182, 30, 121, 114, 182, 141, 1, 148, 34, 128, 251, 11, 254, 14, 224, 103, 200, 206, 6, 52, 46, 0, 18, 36, 177, 49, 192, 247, 253, 95, 1, 127, 15, 188, 31, 91, 42, 138, 184, 38, 6, 184, 164, 152, 83, 47, 245, 100, 22, 7, 113, 180, 1, 206, 236, 227, 228, 30, 137, 193, 12, 12, 67, 222, 30, 3, 226, 104, 0, 138, 51, 191, 188, 62, 138, 54, 197, 182, 37, 177, 128, 129, 182, 200, 227, 92, 128, 35, 180, 82, 90, 113, 16, 166, 13, 240, 11, 61, 108, 75, 124, 197, 148, 88, 198, 192, 88, 228, 87, 83, 136, 101, 0, 106, 53, 0, 47, 161, 253, 208, 211, 36, 177, 152, 129, 58, 228, 253, 118, 64, 189, 1, 25, 130, 160, 134, 128, 19, 206, 56, 208, 215, 13, 144, 100, 132, 129, 49, 40, 135, 122, 3, 50, 2, 126, 70, 224, 41, 180, 19, 249, 246, 51, 162, 244, 205, 139, 193, 222, 192, 108, 128, 174, 67, 191, 134, 160, 235, 249, 226, 232, 35, 180, 137, 155, 1, 237, 61, 1, 18, 178, 46, 92, 76, 116, 63, 32, 37, 23, 7, 7, 209, 14, 30, 0, 78, 3, 36, 57, 98, 160, 37, 202, 58, 17, 224, 254, 131, 50, 4, 249, 228, 128, 117, 127, 9, 32, 201, 49, 3, 124, 45, 152, 9, 108, 7, 100, 8, 242, 193, 1, 247, 156, 156, 5, 116, 0, 36, 98, 160, 192, 64, 63, 252, 165, 183, 64, 134, 32, 187, 70, 224, 99, 212, 239, 207, 129, 94, 128, 68, 12, 148, 100, 128, 91, 57, 253, 26, 224, 160, 144, 122, 4, 217, 224, 128, 138, 207, 58, 29, 10, 72, 196, 64, 32, 6, 190, 140, 80, 50, 4, 118, 27, 0, 41, 126, 160, 166, 174, 64, 149, 24, 224, 252, 239, 159, 1, 122, 53, 176, 199, 24, 124, 128, 250, 210, 19, 191, 82, 171, 214, 181, 170, 25, 224, 24, 193, 124, 96, 35, 160, 87, 3, 51, 57, 216, 130, 186, 225, 60, 15, 238, 34, 45, 17, 3, 177, 48, 192, 239, 19, 92, 5, 60, 1, 236, 7, 100, 12, 210, 229, 128, 117, 192, 111, 71, 76, 5, 122, 0, 18, 49, 144, 8, 3, 156, 71, 112, 14, 176, 24, 224, 151, 141, 101, 8, 146, 229, 128, 175, 100, 139, 0, 46, 248, 106, 13, 72, 196, 64, 106, 12, 244, 70, 202, 51, 128, 149, 64, 3, 32, 99, 16, 15, 7, 123, 14, 115, 60, 29, 255, 249, 74, 38, 9, 201, 64, 139, 144, 247, 235, 246, 98, 6, 184, 29, 217, 25, 192, 119, 129, 139, 0, 14, 32, 178, 167, 32, 169, 157, 1, 110, 193, 181, 14, 224, 118, 240, 156, 178, 251, 60, 32, 137, 136, 1, 25, 128, 136, 136, 44, 17, 77, 59, 156, 251, 42, 112, 62, 112, 33, 48, 24, 104, 3, 72, 252, 25, 224, 123, 61, 149, 158, 187, 240, 252, 30, 120, 17, 224, 18, 93, 73, 196, 12, 200, 0, 68, 76, 104, 153, 232, 218, 226, 60, 39, 161, 140, 1, 104, 16, 56, 199, 64, 3, 86, 32, 193, 35, 239, 227, 152, 74, 255, 8, 176, 6, 224, 102, 28, 251, 0, 73, 140, 12, 200, 0, 196, 72, 110, 133, 168, 251, 227, 26, 87, 159, 125, 11, 224, 78, 198, 116, 91, 117, 6, 242, 36, 159, 162, 176, 155, 129, 167, 129, 229, 0, 21, 254, 13, 64, 146, 32, 3, 50, 0, 9, 146, 93, 38, 41, 142, 17, 12, 4, 134, 0, 35, 128, 51, 1, 26, 136, 147, 129, 44, 9, 61, 37, 175, 3, 84, 244, 213, 192, 107, 192, 38, 224, 0, 32, 73, 137, 1, 25, 128, 148, 136, 247, 73, 182, 23, 174, 211, 179, 240, 23, 0, 7, 18, 217, 91, 56, 9, 232, 9, 180, 7, 76, 150, 93, 200, 28, 93, 116, 239, 2, 84, 240, 103, 129, 245, 192, 54, 224, 45, 64, 98, 16, 3, 50, 0, 6, 85, 134, 79, 86, 186, 227, 58, 13, 0, 13, 65, 63, 128, 175, 13, 125, 0, 26, 11, 46, 105, 238, 4, 124, 9, 224, 120, 67, 156, 194, 247, 114, 118, 223, 119, 2, 31, 1, 59, 0, 206, 192, 35, 234, 1, 62, 233, 105, 0, 56, 15, 95, 98, 56, 3, 50, 0, 134, 87, 80, 128, 236, 209, 245, 232, 26, 0, 26, 129, 142, 0, 123, 15, 39, 2, 110, 175, 225, 4, 28, 211, 56, 240, 122, 59, 128, 198, 164, 121, 221, 127, 142, 115, 159, 0, 156, 195, 64, 165, 166, 2, 127, 8, 188, 7, 80, 161, 183, 1, 111, 3, 124, 194, 83, 249, 105, 4, 104, 0, 36, 22, 51, 240, 255, 64, 223, 185, 14, 106, 236, 138, 225, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130]),
            },
            StackFile {
                path: "components.json".into(),
                content: r###"{
  "$schema": "https://ui.shadcn.com/schema.json",
  "style": "radix-nova",
  "rsc": true,
  "tsx": true,
  "tailwind": {
    "config": "",
    "css": "app/globals.css",
    "baseColor": "neutral",
    "cssVariables": true,
    "prefix": ""
  },
  "iconLibrary": "lucide",
  "rtl": false,
  "aliases": {
    "components": "@/components",
    "utils": "@/lib/utils",
    "ui": "@/components/ui",
    "lib": "@/lib",
    "hooks": "@/hooks"
  },
  "menuColor": "default",
  "menuAccent": "subtle",
  "registries": {}
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "package.json".into(),
                content: r###"{
  "name": "onpkg_next",
  "version": "0.1.0",
  "private": true,
  "scripts": {
    "dev": "next dev",
    "build": "next build",
    "start": "next start",
    "lint": "eslint",
    "db:migrate": "prisma migrate dev",
    "db:generate": "prisma generate",
    "db:seed": "prisma db seed",
    "db:studio": "prisma studio"
  },
  "dependencies": {
    "@aws-sdk/client-s3": "^3.1063.0",
    "@aws-sdk/s3-request-presigner": "^3.1063.0",
    "@hookform/resolvers": "^5.4.0",
    "@prisma/adapter-pg": "^7.8.0",
    "@prisma/client": "^7.8.0",
    "@tanstack/react-query": "^5.101.0",
    "axios": "^1.17.0",
    "bcryptjs": "^3.0.3",
    "class-variance-authority": "^0.7.1",
    "clsx": "^2.1.1",
    "cors": "^2.8.6",
    "ioredis": "^5.11.1",
    "jsonwebtoken": "^9.0.3",
    "lucide-react": "^1.17.0",
    "next": "16.2.7",
    "next-themes": "^0.4.6",
    "node-cron": "^4.2.1",
    "nodemailer": "^8.0.10",
    "pg": "^8.21.0",
    "pino": "^10.3.1",
    "radix-ui": "^1.5.0",
    "react": "19.2.4",
    "react-dom": "19.2.4",
    "react-hook-form": "^7.77.0",
    "shadcn": "^4.10.0",
    "superjson": "^2.2.6",
    "tailwind-merge": "^3.6.0",
    "tw-animate-css": "^1.4.0",
    "zod": "^4.4.3",
    "zustand": "^5.0.14"
  },
  "devDependencies": {
    "@tailwindcss/postcss": "^4",
    "@types/bcryptjs": "^3.0.0",
    "@types/cors": "^2.8.19",
    "@types/jsonwebtoken": "^9.0.10",
    "@types/node": "^20",
    "@types/node-cron": "^3.0.11",
    "@types/nodemailer": "^8.0.0",
    "@types/pg": "^8.20.0",
    "@types/react": "^19",
    "@types/react-dom": "^19",
    "eslint": "^9",
    "eslint-config-next": "16.2.7",
    "pino-pretty": "^13.1.3",
    "prisma": "^7.8.0",
    "tailwindcss": "^4",
    "typescript": "^5"
  },
  "ignoreScripts": [
    "sharp",
    "unrs-resolver"
  ],
  "trustedDependencies": [
    "sharp",
    "unrs-resolver"
  ],
  "prisma": {
    "seed": "bun prisma/seed.ts"
  }
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: ".gitignore".into(),
                content: r###"# See https://help.github.com/articles/ignoring-files/ for more about ignoring files.

# dependencies
/node_modules
/.pnp
.pnp.*
.yarn/*
!.yarn/patches
!.yarn/plugins
!.yarn/releases
!.yarn/versions

# testing
/coverage

# next.js
/.next/
/out/

# production
/build

# misc
.DS_Store
*.pem

# debug
npm-debug.log*
yarn-debug.log*
yarn-error.log*
.pnpm-debug.log*

# env files (can opt-in for committing if needed)
.env*

# vercel
.vercel

# typescript
*.tsbuildinfo
next-env.d.ts

/lib/generated/prisma
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/s3.ts".into(),
                content: r###"import { S3Client, PutObjectCommand, GetObjectCommand } from '@aws-sdk/client-s3';
import { getSignedUrl } from '@aws-sdk/s3-request-presigner';
import { pinoLogger } from './pino-logger';

const S3_REGION = process.env.S3_REGION || 'us-east-1';
const S3_BUCKET = process.env.S3_BUCKET || 'my-app-bucket';
const S3_ACCESS_KEY_ID = process.env.S3_ACCESS_KEY_ID || '';
const S3_SECRET_ACCESS_KEY = process.env.S3_SECRET_ACCESS_KEY || '';
const S3_ENDPOINT = process.env.S3_ENDPOINT; // Supports custom endpoints like MinIO / Cloudflare R2

const s3Client = new S3Client({
  region: S3_REGION,
  credentials: S3_ACCESS_KEY_ID && S3_SECRET_ACCESS_KEY ? {
    accessKeyId: S3_ACCESS_KEY_ID,
    secretAccessKey: S3_SECRET_ACCESS_KEY,
  } : undefined,
  endpoint: S3_ENDPOINT,
});

export { s3Client };

interface UploadParams {
  key: string;
  body: Buffer | Uint8Array;
  contentType: string;
}

export async function uploadToS3({ key, body, contentType }: UploadParams) {
  try {
    const command = new PutObjectCommand({
      Bucket: S3_BUCKET,
      Key: key,
      Body: body,
      ContentType: contentType,
    });

    await s3Client.send(command);
    pinoLogger.info(`Successfully uploaded object to S3: ${key}`);
    return { key, bucket: S3_BUCKET };
  } catch (error) {
    pinoLogger.error(error, `S3 upload error for key "${key}"`);
    throw error;
  }
}

export async function getPresignedDownloadUrl(key: string, expiresInSeconds = 3600) {
  try {
    const command = new GetObjectCommand({
      Bucket: S3_BUCKET,
      Key: key,
    });

    const url = await getSignedUrl(s3Client, command, { expiresIn: expiresInSeconds });
    return url;
  } catch (error) {
    pinoLogger.error(error, `S3 presigned download URL error for key "${key}"`);
    throw error;
  }
}

export async function getPresignedUploadUrl(key: string, contentType: string, expiresInSeconds = 3600) {
  try {
    const command = new PutObjectCommand({
      Bucket: S3_BUCKET,
      Key: key,
      ContentType: contentType,
    });

    const url = await getSignedUrl(s3Client, command, { expiresIn: expiresInSeconds });
    return url;
  } catch (error) {
    pinoLogger.error(error, `S3 presigned upload URL error for key "${key}"`);
    throw error;
  }
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/pino-logger.ts".into(),
                content: r###"import pino from 'pino';

const isDev = process.env.NODE_ENV !== 'production';

export const pinoLogger = pino({
  level: process.env.LOG_LEVEL || 'info',
  transport: isDev
    ? {
        target: 'pino-pretty',
        options: {
          colorize: true,
          translateTime: 'SYS:standard',
          ignore: 'pid,hostname',
        },
      }
    : undefined,
});
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/prisma.ts".into(),
                content: r###"import { PrismaClient } from './generated/prisma/client';
import { PrismaPg } from '@prisma/adapter-pg';
import pg from 'pg';

const { Pool } = pg;

const globalForPrisma = globalThis as unknown as {
  prisma: PrismaClient | undefined;
  pool: pg.Pool | undefined;
};

const connectionString = process.env.DATABASE_URL;

if (!connectionString && process.env.NODE_ENV !== 'production') {
  console.warn('Warning: DATABASE_URL is not set. Database operations will fail.');
}

export let prisma: PrismaClient;

if (process.env.NODE_ENV === 'production') {
  const pool = new Pool({ connectionString });
  const adapter = new PrismaPg(pool);
  prisma = new PrismaClient({ adapter });
} else {
  if (!globalForPrisma.prisma) {
    const pool = new Pool({ connectionString });
    const adapter = new PrismaPg(pool);
    globalForPrisma.pool = pool;
    globalForPrisma.prisma = new PrismaClient({ adapter });
  }
  prisma = globalForPrisma.prisma;
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/auth-utils.ts".into(),
                content: r###"import bcrypt from 'bcryptjs';

export async function hashPassword(password: string): Promise<string> {
  return bcrypt.hash(password, 10);
}

export async function comparePassword(password: string, hash: string): Promise<boolean> {
  return bcrypt.compare(password, hash);
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/utils.ts".into(),
                content: r###"import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/jwt.ts".into(),
                content: r###"import jwt from 'jsonwebtoken';

const JWT_SECRET = process.env.JWT_SECRET || 'super-secret-key-change-me';

interface TokenPayload {
  userId: string;
  email: string;
  role: string;
}

export function signToken(payload: TokenPayload, expiresIn: string = '7d'): string {
  return jwt.sign(payload, JWT_SECRET, { expiresIn: expiresIn as any });
}

export function verifyToken(token: string): TokenPayload | null {
  try {
    return jwt.verify(token, JWT_SECRET) as TokenPayload;
  } catch {
    return null;
  }
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/mailer.ts".into(),
                content: r###"import nodemailer from 'nodemailer';
import { pinoLogger } from './pino-logger';

const SMTP_HOST = process.env.SMTP_HOST || 'smtp.mailtrap.io';
const SMTP_PORT = parseInt(process.env.SMTP_PORT || '2525', 10);
const SMTP_USER = process.env.SMTP_USER || '';
const SMTP_PASS = process.env.SMTP_PASS || '';
const MAIL_FROM = process.env.MAIL_FROM || 'noreply@onpkg.com';

const transporter = nodemailer.createTransport({
  host: SMTP_HOST,
  port: SMTP_PORT,
  secure: SMTP_PORT === 465,
  auth: SMTP_USER && SMTP_PASS ? {
    user: SMTP_USER,
    pass: SMTP_PASS,
  } : undefined,
});

interface SendEmailParams {
  to: string;
  subject: string;
  html: string;
  text?: string;
}

export async function sendEmail({ to, subject, html, text }: SendEmailParams) {
  try {
    const info = await transporter.sendMail({
      from: MAIL_FROM,
      to,
      subject,
      text: text || html.replace(/<[^>]*>/g, ''),
      html,
    });

    pinoLogger.info(`Email sent successfully: ${info.messageId}`);
    return { success: true, messageId: info.messageId };
  } catch (error) {
    pinoLogger.error(error, 'Failed to send email');
    throw error;
  }
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/api-client.ts".into(),
                content: r###"import axios from 'axios';
import { logger } from './logger';

const apiClient = axios.create({
  baseURL: typeof window !== 'undefined' ? '' : process.env.NEXT_PUBLIC_API_URL || 'http://localhost:3000',
  headers: {
    'Content-Type': 'application/json',
  },
});

apiClient.interceptors.request.use(
  (config) => {
    logger.info(`Request: ${config.method?.toUpperCase()} ${config.url}`);
    return config;
  },
  (error) => {
    logger.error('Request Error', error);
    return Promise.reject(error);
  }
);

apiClient.interceptors.response.use(
  (response) => {
    logger.info(`Response: ${response.status} ${response.config.url}`);
    return response;
  },
  (error) => {
    logger.error('Response Error', error.response?.data || error.message);
    return Promise.reject(error);
  }
);

export default apiClient;
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/logger.ts".into(),
                content: r###"type LogLevel = "info" | "warn" | "error" | "debug"

const isDev = process.env.NODE_ENV !== "production"

const styles: Record<LogLevel, string> = {
  info: "color: #3b82f6; font-weight: 600;",
  warn: "color: #f59e0b; font-weight: 600;",
  error: "color: #ef4444; font-weight: 600;",
  debug: "color: #10b981; font-weight: 600;",
}

const formatMessage = (level: LogLevel, message: string) => {
  const prefix = `[APP] ${level.toUpperCase()}`
  const isBrowser = typeof window !== 'undefined'
  if (isBrowser) {
    return [`%c${prefix} %c${message}`, styles[level], "color: inherit; font-weight: normal;"]
  }
  // Server-side terminal color logging
  const colors: Record<LogLevel, string> = {
    info: "\x1b[36m", // Cyan
    warn: "\x1b[33m", // Yellow
    error: "\x1b[31m", // Red
    debug: "\x1b[32m", // Green
  }
  const reset = "\x1b[0m"
  return [`${colors[level]}${prefix}${reset} ${message}`]
}

function log(level: LogLevel, message: string, data?: unknown) {
  if (!isDev && level === "debug") return

  const formatted = formatMessage(level, message)

  if (data === undefined) {
    if (typeof window !== 'undefined') {
      const [prompt, style, reset] = formatted
      if (level === "error") console.error(prompt, style, reset)
      else if (level === "warn") console.warn(prompt, style, reset)
      else console.log(prompt, style, reset)
    } else {
      const [prompt] = formatted
      if (level === "error") console.error(prompt)
      else if (level === "warn") console.warn(prompt)
      else console.log(prompt)
    }
    return
  }

  // Handle data with grouping for a cleaner console
  if (typeof window !== 'undefined') {
    const [prompt, style, reset] = formatted
    console.groupCollapsed(prompt, style, reset)
    if (data instanceof Error) {
      console.error(data.message)
      if (data.stack) console.debug(data.stack)
    } else {
      console.dir(data)
    }
    console.groupEnd()
  } else {
    const [prompt] = formatted
    console.log(prompt, data)
  }
}

export const logger = {
  info: (msg: string, data?: unknown) => log("info", msg, data),
  warn: (msg: string, data?: unknown) => log("warn", msg, data),
  error: (msg: string, data?: unknown) => log("error", msg, data),
  debug: (msg: string, data?: unknown) => log("debug", msg, data),
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/api-logger.ts".into(),
                content: r###"import { NextRequest, NextResponse } from 'next/server';

export function withLogging(
  handler: (request: NextRequest, context?: any) => Promise<NextResponse>
) {
  return async (request: NextRequest, context?: any) => {
    const method = request.method;
    const path = request.nextUrl.pathname;

    // Output incoming request log in cyan
    console.log(`\x1b[36m→\x1b[0m ${method} ${path}`);

    const start = performance.now();
    try {
      const response = await handler(request, context);
      const duration = Math.round(performance.now() - start);

      const statusColor = response.status >= 400 ? '\x1b[31m' : '\x1b[32m';
      console.log(`\x1b[35m←\x1b[0m ${method} ${path} ${statusColor}${response.status}\x1b[0m ${duration}ms`);
      return response;
    } catch (error) {
      const duration = Math.round(performance.now() - start);
      console.log(`\x1b[31m←\x1b[0m ${method} ${path} \x1b[31m500\x1b[0m ${duration}ms`);
      throw error;
    }
  };
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/cors.ts".into(),
                content: r###"import { NextResponse } from 'next/server';

export function corsHeaders(origin = '*') {
  return {
    'Access-Control-Allow-Origin': origin,
    'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type, Authorization',
    'Access-Control-Max-Age': '86400', // 24 hours
  };
}

export function handleOptions() {
  return new NextResponse(null, {
    status: 204,
    headers: corsHeaders(),
  });
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/cron.ts".into(),
                content: r###"import cron from 'node-cron';
import { pinoLogger } from './pino-logger';

export function initCronJobs() {
  pinoLogger.info('Initializing background cron jobs...');

  // Example task: Runs every hour
  cron.schedule('0 * * * *', () => {
    pinoLogger.info('Cron Job [Hourly]: Running system health checks & log cleanup...');
  });

  // Example task: Runs every midnight (00:00)
  cron.schedule('0 0 * * *', () => {
    pinoLogger.info('Cron Job [Daily]: Running database optimization & backup hooks...');
  });

  pinoLogger.info('Background cron jobs registered successfully.');
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/redis.ts".into(),
                content: r###"import Redis from 'ioredis';
import { pinoLogger } from './pino-logger';

const REDIS_URL = process.env.REDIS_URL || 'redis://localhost:6379';

const globalForRedis = globalThis as unknown as {
  redis: Redis | undefined;
};

export let redis: Redis;

if (process.env.NODE_ENV === 'production') {
  redis = new Redis(REDIS_URL);
} else {
  if (!globalForRedis.redis) {
    globalForRedis.redis = new Redis(REDIS_URL, {
      maxRetriesPerRequest: 3,
    });
    pinoLogger.info('Initialized Redis connection (dev)');
  }
  redis = globalForRedis.redis;
}

redis.on('error', (err) => {
  pinoLogger.error(err, 'Redis connection error');
});
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/rate-limiter.ts".into(),
                content: r###"import { redis } from './redis';
import { pinoLogger } from './pino-logger';

interface RateLimitResult {
  success: boolean;
  limit: number;
  remaining: number;
  reset: number;
}

const memoryCache = new Map<string, { tokens: number; lastRefill: number }>();

export async function rateLimit(
  key: string,
  limit = 60,
  windowSeconds = 60
): Promise<RateLimitResult> {
  const now = Math.floor(Date.now() / 1000);
  const redisKey = `ratelimit:${key}`;

  try {
    const pipeline = redis.pipeline();
    pipeline.incr(redisKey);
    pipeline.ttl(redisKey);
    const results = await pipeline.exec();

    if (results) {
      const count = results[0][1] as number;
      const ttl = results[1][1] as number;

      if (count === 1) {
        await redis.expire(redisKey, windowSeconds);
      }

      const isAllowed = count <= limit;
      return {
        success: isAllowed,
        limit,
        remaining: Math.max(0, limit - count),
        reset: now + (ttl > 0 ? ttl : windowSeconds),
      };
    }
  } catch (error) {
    pinoLogger.warn(error, 'Redis rate limiting failed. Falling back to in-memory limiting.');
  }

  // Fallback: In-memory sliding window rate limiter
  const bucket = memoryCache.get(key) || { tokens: limit, lastRefill: now };
  const refillRate = limit / windowSeconds;
  const elapsed = now - bucket.lastRefill;

  const currentTokens = Math.min(limit, bucket.tokens + elapsed * refillRate);

  if (currentTokens >= 1) {
    memoryCache.set(key, {
      tokens: currentTokens - 1,
      lastRefill: now,
    });
    return {
      success: true,
      limit,
      remaining: Math.floor(currentTokens - 1),
      reset: now + windowSeconds,
    };
  } else {
    memoryCache.set(key, {
      tokens: currentTokens,
      lastRefill: now,
    });
    return {
      success: false,
      limit,
      remaining: 0,
      reset: now + windowSeconds,
    };
  }
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "prisma.config.ts".into(),
                content: r###"// This file was generated by Prisma, and assumes you have installed the following:
// npm install --save-dev prisma dotenv
import "dotenv/config";
import { defineConfig } from "prisma/config";

export default defineConfig({
  schema: "prisma/schema.prisma",
  migrations: {
    path: "prisma/migrations",
    seed: "bun prisma/seed.ts",
  },
  datasource: {
    url: process.env["DATABASE_URL"],
  },
});
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "store/useAppStore.ts".into(),
                content: r###"import { create } from 'zustand';
import { persist } from 'zustand/middleware';

interface UserInfo {
  id: string;
  name: string | null;
  email: string;
  role: 'ADMIN' | 'USER' | 'GUEST';
}

interface AppState {
  user: UserInfo | null;
  setUser: (user: UserInfo | null) => void;
  isLoading: boolean;
  setIsLoading: (loading: boolean) => void;
}

export const useAppStore = create<AppState>()(
  persist(
    (set) => ({
      user: null,
      setUser: (user) => set({ user }),
      isLoading: false,
      setIsLoading: (isLoading) => set({ isLoading }),
    }),
    {
      name: 'app-storage',
      skipHydration: true, // Safe for Next.js SSR hydration
    }
  )
);
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "public/vercel.svg".into(),
                content: r###"<svg fill="none" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1155 1000"><path d="m577.3 0 577.4 1000H0z" fill="#fff"/></svg>"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "public/window.svg".into(),
                content: r###"<svg fill="none" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path fill-rule="evenodd" clip-rule="evenodd" d="M1.5 2.5h13v10a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1zM0 1h16v11.5a2.5 2.5 0 0 1-2.5 2.5h-11A2.5 2.5 0 0 1 0 12.5zm3.75 4.5a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5M7 4.75a.75.75 0 1 1-1.5 0 .75.75 0 0 1 1.5 0m1.75.75a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5" fill="#666"/></svg>"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "public/next.svg".into(),
                content: r###"<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 394 80"><path fill="#000" d="M262 0h68.5v12.7h-27.2v66.6h-13.6V12.7H262V0ZM149 0v12.7H94v20.4h44.3v12.6H94v21h55v12.6H80.5V0h68.7zm34.3 0h-17.8l63.8 79.4h17.9l-32-39.7 32-39.6h-17.9l-23 28.6-23-28.6zm18.3 56.7-9-11-27.1 33.7h17.8l18.3-22.7z"/><path fill="#000" d="M81 79.3 17 0H0v79.3h13.6V17l50.2 62.3H81Zm252.6-.4c-1 0-1.8-.4-2.5-1s-1.1-1.6-1.1-2.6.3-1.8 1-2.5 1.6-1 2.6-1 1.8.3 2.5 1a3.4 3.4 0 0 1 .6 4.3 3.7 3.7 0 0 1-3 1.8zm23.2-33.5h6v23.3c0 2.1-.4 4-1.3 5.5a9.1 9.1 0 0 1-3.8 3.5c-1.6.8-3.5 1.3-5.7 1.3-2 0-3.7-.4-5.3-1s-2.8-1.8-3.7-3.2c-.9-1.3-1.4-3-1.4-5h6c.1.8.3 1.6.7 2.2s1 1.2 1.6 1.5c.7.4 1.5.5 2.4.5 1 0 1.8-.2 2.4-.6a4 4 0 0 0 1.6-1.8c.3-.8.5-1.8.5-3V45.5zm30.9 9.1a4.4 4.4 0 0 0-2-3.3 7.5 7.5 0 0 0-4.3-1.1c-1.3 0-2.4.2-3.3.5-.9.4-1.6 1-2 1.6a3.5 3.5 0 0 0-.3 4c.3.5.7.9 1.3 1.2l1.8 1 2 .5 3.2.8c1.3.3 2.5.7 3.7 1.2a13 13 0 0 1 3.2 1.8 8.1 8.1 0 0 1 3 6.5c0 2-.5 3.7-1.5 5.1a10 10 0 0 1-4.4 3.5c-1.8.8-4.1 1.2-6.8 1.2-2.6 0-4.9-.4-6.8-1.2-2-.8-3.4-2-4.5-3.5a10 10 0 0 1-1.7-5.6h6a5 5 0 0 0 3.5 4.6c1 .4 2.2.6 3.4.6 1.3 0 2.5-.2 3.5-.6 1-.4 1.8-1 2.4-1.7a4 4 0 0 0 .8-2.4c0-.9-.2-1.6-.7-2.2a11 11 0 0 0-2.1-1.4l-3.2-1-3.8-1c-2.8-.7-5-1.7-6.6-3.2a7.2 7.2 0 0 1-2.4-5.7 8 8 0 0 1 1.7-5 10 10 0 0 1 4.3-3.5c2-.8 4-1.2 6.4-1.2 2.3 0 4.4.4 6.2 1.2 1.8.8 3.2 2 4.3 3.4 1 1.4 1.5 3 1.5 5h-5.8z"/></svg>"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "public/file.svg".into(),
                content: r###"<svg fill="none" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg"><path d="M14.5 13.5V5.41a1 1 0 0 0-.3-.7L9.8.29A1 1 0 0 0 9.08 0H1.5v13.5A2.5 2.5 0 0 0 4 16h8a2.5 2.5 0 0 0 2.5-2.5m-1.5 0v-7H8v-5H3v12a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1M9.5 5V2.12L12.38 5zM5.13 5h-.62v1.25h2.12V5zm-.62 3h7.12v1.25H4.5zm.62 3h-.62v1.25h7.12V11z" clip-rule="evenodd" fill="#666" fill-rule="evenodd"/></svg>"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "public/globe.svg".into(),
                content: r###"<svg fill="none" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><g clip-path="url(#a)"><path fill-rule="evenodd" clip-rule="evenodd" d="M10.27 14.1a6.5 6.5 0 0 0 3.67-3.45q-1.24.21-2.7.34-.31 1.83-.97 3.1M8 16A8 8 0 1 0 8 0a8 8 0 0 0 0 16m.48-1.52a7 7 0 0 1-.96 0H7.5a4 4 0 0 1-.84-1.32q-.38-.89-.63-2.08a40 40 0 0 0 3.92 0q-.25 1.2-.63 2.08a4 4 0 0 1-.84 1.31zm2.94-4.76q1.66-.15 2.95-.43a7 7 0 0 0 0-2.58q-1.3-.27-2.95-.43a18 18 0 0 1 0 3.44m-1.27-3.54a17 17 0 0 1 0 3.64 39 39 0 0 1-4.3 0 17 17 0 0 1 0-3.64 39 39 0 0 1 4.3 0m1.1-1.17q1.45.13 2.69.34a6.5 6.5 0 0 0-3.67-3.44q.65 1.26.98 3.1M8.48 1.5l.01.02q.41.37.84 1.31.38.89.63 2.08a40 40 0 0 0-3.92 0q.25-1.2.63-2.08a4 4 0 0 1 .85-1.32 7 7 0 0 1 .96 0m-2.75.4a6.5 6.5 0 0 0-3.67 3.44 29 29 0 0 1 2.7-.34q.31-1.83.97-3.1M4.58 6.28q-1.66.16-2.95.43a7 7 0 0 0 0 2.58q1.3.27 2.95.43a18 18 0 0 1 0-3.44m.17 4.71q-1.45-.12-2.69-.34a6.5 6.5 0 0 0 3.67 3.44q-.65-1.27-.98-3.1" fill="#666"/></g><defs><clipPath id="a"><path fill="#fff" d="M0 0h16v16H0z"/></clipPath></defs></svg>"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "hooks/useAuth.ts".into(),
                content: r###"import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import apiClient from '@/lib/api-client';
import { useAppStore } from '@/store/useAppStore';
import { LoginFormValues, RegisterFormValues, User } from '@/types/schema';
import { logger } from '@/lib/logger';

export const useAuth = () => {
  const queryClient = useQueryClient();
  const { setUser, user } = useAppStore();

  const meQuery = useQuery({
    queryKey: ['auth-me'],
    queryFn: async () => {
      try {
        const { data } = await apiClient.get<{ success: boolean; data: User }>('/api/auth/me');
        if (data.success) {
          setUser(data.data);
          return data.data;
        }
        return null;
      } catch (error) {
        logger.debug('No active session found');
        setUser(null);
        return null;
      }
    },
    retry: false,
  });

  const loginMutation = useMutation({
    mutationFn: async (credentials: LoginFormValues) => {
      const { data } = await apiClient.post<{ success: boolean; data: User; token: string }>('/api/auth/login', credentials);
      if (typeof window !== 'undefined' && data.token) {
        localStorage.setItem('auth-token', data.token);
      }
      return data;
    },
    onSuccess: (data) => {
      setUser(data.data);
      queryClient.invalidateQueries({ queryKey: ['auth-me'] });
    },
  });

  const registerMutation = useMutation({
    mutationFn: async (userData: RegisterFormValues) => {
      const { data } = await apiClient.post<{ success: boolean; data: User; token: string }>('/api/auth/register', userData);
      if (typeof window !== 'undefined' && data.token) {
        localStorage.setItem('auth-token', data.token);
      }
      return data;
    },
    onSuccess: (data) => {
      setUser(data.data);
      queryClient.invalidateQueries({ queryKey: ['auth-me'] });
    },
  });

  const logoutMutation = useMutation({
    mutationFn: async () => {
      await apiClient.post('/api/auth/me');
      if (typeof window !== 'undefined') {
        localStorage.removeItem('auth-token');
      }
    },
    onSuccess: () => {
      setUser(null);
      queryClient.setQueryData(['auth-me'], null);
      queryClient.invalidateQueries({ queryKey: ['auth-me'] });
    },
  });

  return {
    user,
    isLoading: meQuery.isLoading,
    login: loginMutation.mutateAsync,
    isLoggingIn: loginMutation.isPending,
    register: registerMutation.mutateAsync,
    isRegistering: registerMutation.isPending,
    logout: logoutMutation.mutateAsync,
    isLoggingOut: logoutMutation.isPending,
  };
};
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "hooks/usePosts.ts".into(),
                content: r###"import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import apiClient from '@/lib/api-client';
import { Post, CreatePostValues } from '@/types/schema';

export const usePosts = () => {
  const queryClient = useQueryClient();

  const postsQuery = useQuery({
    queryKey: ['posts'],
    queryFn: async () => {
      const { data } = await apiClient.get<{ success: boolean; data: (Post & { author: { name: string | null; email: string } })[] }>('/api/posts');
      return data.data;
    },
  });

  const createPostMutation = useMutation({
    mutationFn: async (newPost: CreatePostValues & { token: string }) => {
      const { token, ...postData } = newPost;
      const { data } = await apiClient.post<{ success: boolean; data: Post }>('/api/posts', postData, {
        headers: {
          Authorization: `Bearer ${token}`,
        },
      });
      return data.data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['posts'] });
    },
  });

  return {
    posts: postsQuery.data || [],
    isLoading: postsQuery.isLoading,
    error: postsQuery.error,
    createPost: createPostMutation.mutateAsync,
    isCreating: createPostMutation.isPending,
  };
};
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "AGENTS.md".into(),
                content: r###"<!-- BEGIN:nextjs-agent-rules -->
# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` before writing any code. Heed deprecation notices.
<!-- END:nextjs-agent-rules -->
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "docs/LOGGING.md".into(),
                content: r###"# Structured Logging with Pino 🪵

This backend template integrates **Pino**, a high-performance, structured logging framework.

## 🚀 Why Pino?

- **Speed**: Pino is one of the fastest loggers in the Node.js ecosystem, with negligible latency impact.
- **Structured JSON**: Logs are printed as JSON in production, enabling easy ingestion by log managers (like Datadog, Logtail, Elasticsearch, or AWS CloudWatch).
- **Colorized Dev Mode**: Uses `pino-pretty` to print readable logs during local development.

---

## 🛠️ Usage Guide

Import the configured `pinoLogger` from `@/lib/pino-logger`:

```typescript
import { pinoLogger } from '@/lib/pino-logger';

// Standard logs
pinoLogger.info('App successfully initialized');
pinoLogger.warn('Rate limit threshold reached for IP: 127.0.0.1');

// Logs with metadata objects
pinoLogger.info({ userId: '123', action: 'CREATE_POST' }, 'User created a new post');

// Error logging
try {
  throw new Error('Database connection failed');
} catch (error) {
  pinoLogger.error(error, 'An unexpected error occurred during seeding');
}
```

---

## 🎛️ Configurations

Adjust your log settings in `.env`:
```env
# Supported: fatal, error, warn, info, debug, trace
LOG_LEVEL="info"
```
During production builds (`NODE_ENV=production`), `pino-pretty` is automatically bypassed to output raw JSON strings to `stdout` for optimal performance.
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "docs/CRON.md".into(),
                content: r###"# Background Task Scheduling with Node-Cron ⏰

This backend template supports scheduled cron tasks using **Node-Cron**, allowing you to execute database operations, backups, health checks, or log cleanup on specific time intervals.

## 🛠️ Usage Guide

### 1. Registering Tasks
Tasks are registered and managed inside `@/lib/cron.ts`. It utilizes standard 5-field cron syntax:
`* * * * *` (minute hour day-of-month month day-of-week).

```typescript
import cron from 'node-cron';
import { pinoLogger } from './pino-logger';

export function initCronJobs() {
  pinoLogger.info('Initializing background cron jobs...');

  // E.g. Run every hour
  cron.schedule('0 * * * *', () => {
    pinoLogger.info('Cron task: Running hourly database cleanups...');
  });
}
```

---

### 2. Startup Hooks in Next.js
To ensure background task runners register automatically on Next.js server boot, you should initialize them inside `instrumentation.ts` in the root of the project. Next.js 16 calls the `register()` function once when the runtime starts.

Create an `instrumentation.ts` file in your root folder:

```typescript
export async function register() {
  // Only register background schedulers on the server side
  if (process.env.NEXT_RUNTIME === 'nodejs') {
    const { initCronJobs } = await import('./lib/cron');
    initCronJobs();
  }
}
```

Make sure to enable instrumentation in your `next.config.ts`:
```typescript
import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
  experimental: {
    instrumentationHook: true,
  },
};

export default nextConfig;
```
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "docs/STORAGE.md".into(),
                content: r###"# Object File Storage with S3 & Cloudflare R2 🪣

This backend template utilizes the official **AWS SDK v3 S3 client** (`@aws-sdk/client-s3` and `@aws-sdk/s3-request-presigner`) to connect to object storage buckets. This architecture is fully compatible with standard AWS S3, Cloudflare R2, MinIO, or DigitalOcean Spaces.

## 🛠️ Usage Guide

### 1. Direct Server-Side Upload
Upload files directly from your server actions or API routes:

```typescript
import { uploadToS3 } from '@/lib/s3';

const fileBuffer = Buffer.from('Hello File Storage');

await uploadToS3({
  key: 'uploads/docs/hello-world.txt',
  body: fileBuffer,
  contentType: 'text/plain',
});
```

---

### 2. Client-Side Uploads via Presigned URLs (Best Practice)
For optimal speed, large file uploads should bypass the Next.js server limits and be uploaded directly from the browser to the bucket using a secure, pre-signed upload URL.

#### Step A: Generate URL in Route Handler / Server Action
```typescript
import { getPresignedUploadUrl } from '@/lib/s3';

// Generate a URL valid for 10 minutes (600 seconds)
const uploadUrl = await getPresignedUploadUrl('user-avatars/user_1.png', 'image/png', 600);
```

#### Step B: Send file directly from Client Component
```typescript
const file = event.target.files[0];

await fetch(uploadUrl, {
  method: 'PUT',
  body: file,
  headers: {
    'Content-Type': file.type,
  },
});
```

---

## 🎛️ Configurations

Adjust the environment variables in `.env`:
```env
S3_REGION="us-east-1"
S3_BUCKET="my-app-bucket"
S3_ACCESS_KEY_ID="your-access-key-id"
S3_SECRET_ACCESS_KEY="your-secret-access-key"

# Optional: Add custom endpoints for MinIO / Cloudflare R2 / DigitalOcean Spaces
S3_ENDPOINT="https://xxxxxx.r2.cloudflarestorage.com"
```
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "docs/MAILER.md".into(),
                content: r###"# SMTP Mailer with Nodemailer ✉️

This backend starter templates uses **Nodemailer** to dispatch transactional emails (e.g., account verifications, password resets, welcome emails).

## 🛠️ Usage Guide

Import the `sendEmail` helper function from `@/lib/mailer`:

```typescript
import { sendEmail } from '@/lib/mailer';

try {
  await sendEmail({
    to: 'user@example.com',
    subject: 'Welcome to Onpkg Next.js Template!',
    html: '<h1>Account Created</h1><p>Your fullstack template account has been successfully set up.</p>',
    text: 'Account Created. Your fullstack template account has been successfully set up.', // Optional text-only fallback
  });
} catch (error) {
  // Handle mail dispatch error
}
```

---

## 🎛️ Configurations

Update mail settings in `.env` to connect with your SMTP provider (e.g., Mailtrap, SendGrid, Amazon SES, Resend):

```env
# SMTP connection options
SMTP_HOST="smtp.mailtrap.io"
SMTP_PORT=2525
SMTP_USER="your-smtp-username"
SMTP_PASS="your-smtp-password"

# Sender settings
MAIL_FROM="noreply@onpkg.com"
```
In case `SMTP_USER` and `SMTP_PASS` variables are omitted, Nodemailer will attempt to send emails over a local direct SMTP connection.
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "docs/PRISMA.md".into(),
                content: r###"# Prisma 7 Database Documentation 🗄️

This template uses **Prisma v7**, which introduces several architectural changes, specifically separation of concerns between CLI operations and runtime database connections.

## ⚠️ Key Differences in Prisma 7

### 1. No Connection URL in `schema.prisma`
In previous Prisma versions, you defined `url = env("DATABASE_URL")` directly inside the `datasource` block of `schema.prisma`. In Prisma 7, this has been deprecated.
The datasource block is now simplified:
```prisma
datasource db {
  provider = "postgresql"
}
```

### 2. Connection URL inside `prisma.config.ts`
All CLI commands (like migrations and DB pushes) pull the database configuration from `prisma.config.ts` in the root of the project:
```typescript
import "dotenv/config";
import { defineConfig, env } from "prisma/config";

export default defineConfig({
  schema: "prisma/schema.prisma",
  migrations: {
    path: "prisma/migrations",
  },
  datasource: {
    url: env("DATABASE_URL"),
  },
});
```

### 3. Native JS Driver Adapters at Runtime
Prisma 7 removes the bundled Rust query engine binary by default to keep package size minimal and make it serverless/edge-ready.
Instead, it requires passing a **Driver Adapter** (e.g., node-postgres, serverless pg, or neon) when instantiating the client:
```typescript
import { PrismaClient } from './generated/prisma/client';
import { PrismaPg } from '@prisma/adapter-pg';
import pg from 'pg';

const pool = new pg.Pool({ connectionString: process.env.DATABASE_URL });
const adapter = new PrismaPg(pool);
const prisma = new PrismaClient({ adapter });
```

---

## 🛠️ PostgreSQL & Database Operations

This template provides a Docker-based PostgreSQL setup to get you up and running instantly.

### 1. Start PostgreSQL Container
Spin up a local PostgreSQL 16 server in the background:
```bash
docker-compose up -d
```
This starts a Postgres instance mapped to port `5432` with username `postgres`, password `postgres`, and database `onpkg_db`.

### 2. Run Database Migrations
Create database tables and schemas:
```bash
bun run db:migrate
```

### 3. Generate Type-Safe Client
Re-generate the custom Prisma Client under `lib/generated/prisma`:
```bash
bun run db:generate
```

### 4. Seed Database
Populate database with mock users and posts (with hashed passwords):
```bash
bun run db:seed
```

### 5. Open Prisma Studio
Explore and edit database tables in an interactive browser GUI:
```bash
bun run db:studio
```

"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "docs/ARCHITECTURE.md".into(),
                content: r###"# Template Architecture & Layout 🏗️

This project template is designed for building highly performant full-stack web applications with Next.js 16, TypeScript, Bun, and Prisma.

## 📂 Folder Structure

```text
├── app/                  # Next.js App Router routes & layouts
│   ├── actions/          # Next.js Server Actions (if any)
│   ├── api/              # API endpoints (auth, posts, seed)
│   ├── favicon.ico       # Favicon asset
│   ├── globals.css       # Global styles (Tailwind CSS v4 + tw-animate-css)
│   ├── layout.tsx        # Main application layout wrapping Providers
│   └── page.tsx          # Homepage UI feed and stack sandbox
├── components/           # Reusable UI & helper components
│   ├── ui/               # Lower-level Shadcn UI primitives (button, card, input)
│   ├── ThemeToggle.tsx   # SSR-safe client theme switcher button
│   ├── providers.tsx     # Consolidated context wrapper (Query, next-themes, Zustand)
│   └── Navbar.tsx        # Top navbar containing modal authorization
├── docs/                 # Architectural & configuration guides
├── hooks/                # React Query data fetching hooks (useAuth, usePosts)
├── lib/                  # Server-side & client-side utilities
│   ├── generated/prisma  # Custom target path for type-safe Prisma client
│   ├── api-client.ts     # Request interceptor Axios client
│   ├── auth-utils.ts     # Bcryptjs password hashing helpers
│   ├── jwt.ts            # Sign & verify tokens
│   ├── logger.ts         # Cross-environment terminal/console logger
│   ├── prisma.ts         # Singleton database connection client
│   └── utils.ts          # Tailwind cn utility function
├── prisma/               # Database schemas & seeds
├── store/                # Hydration-safe Zustand state stores
└── types/                # Zod schemas & shared TypeScript types
```

---

## 🔒 Authentication Flow

Authentication is handled via state-of-the-art secure JWTs.

1. **Registration/Login**: User posts credentials to `/api/auth/register` or `/api/auth/login`.
2. **Password Validation**: Passwords are hashed and checked using `bcryptjs` on the server.
3. **Session Issuing**: A JWT is generated containing the user info (`userId`, `email`, `role`) and sent back.
4. **Cookie & Storage Sync**:
   - The server sets the JWT as an `httpOnly` secure `sameSite=strict` cookie.
   - The client stores the JWT token in `localStorage` as fallback for authorization headers.
5. **State Sync**: The client hooks `useAuth` query `/api/auth/me` to fetch current user data and caches it into the **Zustand global store** `useAppStore` for instant page shell rendering.
6. **Log Out**: POSTing to `/api/auth/me` expires the browser cookie and wipes client memory stores.

---

## ⚛️ State Management & Data Fetching

### 1. Hydration-Safe Zustand
Next.js server-side renders (SSR) pages first. If Zustand loads persisted local storage state during SSR, a client-server mismatch warning triggers.
This template uses `skipHydration: true` on store definition, and manually triggers rehydration inside a client-side `useEffect` in the consolidated provider layout wrapper:
```typescript
useEffect(() => {
  useAppStore.persist.rehydrate();
}, []);
```

### 2. React Query Integration
Data fetching is managed by TanStack Query. It handles queries, mutations, cache invalidation, loading states, and API errors smoothly. The custom hooks (`useAuth` and `usePosts`) abstract API routing from the component layer.
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "docs/REDIS.md".into(),
                content: r###"# Redis Cache & Rate Limiting 🚀

This template integrates **Redis** (via the `ioredis` package) to support high-performance caching, key-value storage, and API rate limiting.

## 🛠️ Usage Guide

### 1. Direct Redis Access
Import the shared singleton connection from `@/lib/redis`:

```typescript
import { redis } from '@/lib/redis';

// Set values with custom Expiration (TTL)
await redis.set('cache:users:list', JSON.stringify(users), 'EX', 3600); // 1 hour TTL

// Get values
const cachedData = await redis.get('cache:users:list');
if (cachedData) {
  const users = JSON.parse(cachedData);
}

// Delete key
await redis.del('cache:users:list');
```

---

## 🛡️ API Rate Limiting

The template provides a custom sliding-window token bucket rate limiter in `@/lib/rate-limiter`. It includes an automatic **in-memory memory cache fallback** in case Redis is not active, making it fully functional in local dev environments even without Redis.

### Usage in Route Handlers
```typescript
import { NextRequest, NextResponse } from 'next/server';
import { rateLimit } from '@/lib/rate-limiter';

export async function GET(request: NextRequest) {
  const ip = request.headers.get('x-forwarded-for') || '127.0.0.1';

  // Limit to 30 requests per minute per IP
  const { success, limit, remaining, reset } = await rateLimit(`ip:${ip}`, 30, 60);

  if (!success) {
    return NextResponse.json(
      { error: 'Too Many Requests' },
      {
        status: 429,
        headers: {
          'X-RateLimit-Limit': limit.toString(),
          'X-RateLimit-Remaining': remaining.toString(),
          'X-RateLimit-Reset': reset.toString(),
        }
      }
    );
  }

  // Handle standard route business logic...
}
```

---

## 🎛️ Configurations

Update connection settings in `.env`:
```env
REDIS_URL="redis://localhost:6379"
```
To run a local Redis instance using Docker, add the Redis service to your `docker-compose.yml`.
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "instrumentation.ts".into(),
                content: r###"export async function register() {
  if (process.env.NEXT_RUNTIME === 'nodejs') {
    const { initCronJobs } = await import('./lib/cron');
    initCronJobs();
  }
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "prisma/schema.prisma".into(),
                content: r###"// This is your Prisma schema file,
// learn more about it in the docs: https://pris.ly/d/prisma-schema

generator client {
  provider = "prisma-client"
  output   = "../lib/generated/prisma"
}

datasource db {
  provider = "postgresql"
}

enum Role {
  ADMIN
  USER
  GUEST
}

model User {
  id        String   @id @default(uuid())
  email     String   @unique
  name      String?
  password  String   // Hashed password
  role      Role     @default(USER)
  createdAt DateTime @default(now())
  updatedAt DateTime @updatedAt
  posts     Post[]
}

model Post {
  id        String   @id @default(uuid())
  title     String
  content   String?
  published Boolean  @default(false)
  views     Int      @default(0)
  createdAt DateTime @default(now())
  updatedAt DateTime @updatedAt
  authorId  String
  author    User     @relation(fields: [authorId], references: [id], onDelete: Cascade)
}
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "prisma/seed.ts".into(),
                content: r###"import { PrismaClient } from '../lib/generated/prisma/client';
import { PrismaPg } from '@prisma/adapter-pg';
import pg from 'pg';
import bcrypt from 'bcryptjs';

const { Pool } = pg;
const connectionString = process.env.DATABASE_URL;

if (!connectionString) {
  console.error("DATABASE_URL is required to seed the database.");
  process.exit(1);
}

const pool = new Pool({ connectionString });
const adapter = new PrismaPg(pool);
const prisma = new PrismaClient({ adapter });

async function main() {
  console.log('🌱 Seeding database...');

  // Clean the database
  await prisma.post.deleteMany();
  await prisma.user.deleteMany();

  // Create Users
  const adminPassword = await bcrypt.hash('admin123', 10);
  const userPassword = await bcrypt.hash('user123', 10);

  const admin = await prisma.user.create({
    data: {
      email: 'admin@onpkg.com',
      name: 'Admin User',
      password: adminPassword,
      role: 'ADMIN',
    },
  });

  const user1 = await prisma.user.create({
    data: {
      email: 'user1@onpkg.com',
      name: 'Aswin Dev',
      password: userPassword,
      role: 'USER',
    },
  });

  const user2 = await prisma.user.create({
    data: {
      email: 'user2@onpkg.com',
      name: 'Jane Smith',
      password: userPassword,
      role: 'USER',
    },
  });

  // Create Posts
  await prisma.post.createMany({
    data: [
      {
        title: 'Getting Started with Next.js 16 and Prisma 7',
        content: 'Next.js 16 and Prisma 7 provide an incredibly powerful combo for full-stack React applications. By combining Next.js Server Actions with Prisma driver adapters, you can build blazing fast, edge-ready applications.',
        published: true,
        authorId: user1.id,
        views: 120,
      },
      {
        title: 'Building Beautiful Interfaces with Tailwind CSS v4',
        content: 'Tailwind CSS v4 introduces a streamlined engine, CSS-first configuration, and native cascading layers. It makes managing design systems a breeze without the bloat of traditional CSS setups.',
        published: true,
        authorId: user1.id,
        views: 340,
      },
      {
        title: 'The Future of State Management with Zustand',
        content: 'Zustand is a small, fast, and scalable bear-bones state-management solution. It has a comfy API based on hooks, is not opinionated, and doesn\'t wrap your app in providers.',
        published: true,
        authorId: user2.id,
        views: 95,
      },
      {
        title: 'Draft post',
        content: 'This is a draft post that is not published yet.',
        published: false,
        authorId: admin.id,
      },
    ],
  });

  console.log('✅ Database seeded successfully!');
}

main()
  .catch((e) => {
    console.error('❌ Error seeding database:', e);
    process.exit(1);
  })
  .finally(async () => {
    await pool.end();
  });
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "types/schema.ts".into(),
                content: r###"import { z } from 'zod';

export const RoleEnum = z.enum(['ADMIN', 'USER', 'GUEST']);
export type Role = z.infer<typeof RoleEnum>;

export const UserSchema = z.object({
  id: z.string().uuid(),
  email: z.string().email('Invalid email address'),
  name: z.string().min(2, 'Name must be at least 2 characters').nullable(),
  role: RoleEnum,
  createdAt: z.string().or(z.date()),
  updatedAt: z.string().or(z.date()),
});

export type User = z.infer<typeof UserSchema>;

export const PostSchema = z.object({
  id: z.string().uuid(),
  title: z.string().min(3, 'Title must be at least 3 characters'),
  content: z.string().min(5, 'Content must be at least 5 characters').nullable(),
  published: z.boolean().default(false),
  views: z.number().int().nonnegative().default(0),
  createdAt: z.string().or(z.date()),
  updatedAt: z.string().or(z.date()),
  authorId: z.string().uuid(),
});

export type Post = z.infer<typeof PostSchema>;

export const LoginFormSchema = z.object({
  email: z.string().email('Invalid email address'),
  password: z.string().min(6, 'Password must be at least 6 characters'),
});

export type LoginFormValues = z.infer<typeof LoginFormSchema>;

export const RegisterFormSchema = z.object({
  email: z.string().email('Invalid email address'),
  name: z.string().min(2, 'Name must be at least 2 characters'),
  password: z.string().min(6, 'Password must be at least 6 characters'),
});

export type RegisterFormValues = z.infer<typeof RegisterFormSchema>;

export const CreatePostSchema = z.object({
  title: z.string().min(3, 'Title must be at least 3 characters'),
  content: z.string().min(5, 'Content must be at least 5 characters'),
  published: z.boolean().default(false),
});

export type CreatePostValues = z.infer<typeof CreatePostSchema>;
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "docker-compose.yml".into(),
                content: r###"version: '3.8'

services:
  postgres:
    image: postgres:16-alpine
    container_name: onpkg-postgres
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: onpkg_db
    ports:
      - '5434:5432'
    volumes:
      - postgres_data:/var/lib/postgresql/data
    restart: always

volumes:
  postgres_data:
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "postcss.config.mjs".into(),
                content: r###"const config = {
  plugins: {
    "@tailwindcss/postcss": {},
  },
};

export default config;
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "CLAUDE.md".into(),
                content: r###"@AGENTS.md
"###.into(),
                binary_content: None,
            },
        ],
    }
}
