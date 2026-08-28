#![allow(clippy::all, unused)]

use crate::tools::onpkg::stacks::{Stack, StackFile};

pub fn hono_api() -> Stack {
    Stack {
        hooks: vec![],
        name: "hono-api".into(),
        runtime: "bun".into(),
        description: "Hono API with Bun and Pino logger".into(),
        packages: vec!["hono".into(), "pino".into()],
        dev_packages: vec![
            "typescript".into(),
            "@types/node".into(),
            "pino-pretty".into(),
        ],
        transitive_packages: vec![],
        files: vec![
            StackFile {
                path: "src/server.ts".into(),
                content: r##"import { Hono } from "hono";
import { logger as httpLogger } from "hono/logger";
import { logger } from "./utils/logger";

const app = new Hono();
const PORT = process.env.PORT || 3000;

app.use("*", httpLogger());
app.get("/", (c) => c.text(`running on ${PORT}`));

export default {
  port: PORT,
  fetch: app.fetch,
};
"##
                .into(),
                binary_content: None,
            },
            StackFile {
                path: "src/utils/logger.ts".into(),
                content: r##"import pino from "pino";

const isProd = process.env.NODE_ENV === "production";

export const logger = pino({
  level: isProd ? "info" : "debug",
  transport: !isProd ? {
    target: "pino-pretty",
    options: {
      colorize: true,
      translateTime: "SYS:standard",
      ignore: "pid,hostname",
    },
  } : undefined,
});
"##
                .into(),
                binary_content: None,
            },
        ],
    }
}

pub fn hono_full() -> Stack {
    Stack {
        hooks: vec![],
        name: "hono-full".into(),
        runtime: "bun".into(),
        description: "Hono + Prisma + Zod + Pino + Better Auth (Prisma Adapter)".into(),
        packages: vec![
            "hono".into(),
            "better-auth".into(),
            "@prisma/client".into(),
            "zod".into(),
            "pino".into(),
            "@prisma/adapter-pg".into(),
            "dotenv".into(),
            "pg".into(),
        ],
        dev_packages: vec![
            "typescript".into(),
            "@types/node".into(),
            "pino-pretty".into(),
            "prisma".into(),
            "@types/pg".into(),
        ],
        transitive_packages: vec![],
        files: vec![
            StackFile {
                path: "package.json".into(),
                content: r##"{
  "name": "hono-full",
  "version": "1.0.0",
  "scripts": {
    "dev": "bun --watch src/server.ts",
    "start": "bun run src/server.ts",
    "lint": "echo \"no lint configured\"",
    "format": "echo \"no formatter configured\""
  },
  "dependencies": {
    "hono": "",
    "better-auth": "",
    "@prisma/client": "",
    "zod": "",
    "pino": "",
    "@prisma/adapter-pg": "",
    "dotenv": "",
    "pg": ""
  },
  "devDependencies": {
    "typescript": "",
    "@types/node": "",
    "prisma": "",
    "pino-pretty": "",
    "@types/pg": ""
  }
}
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "src/server.ts".into(),
                content: r##"import { Hono } from "hono";
import { logger as httpLogger } from "hono/logger";
import { cors } from "hono/cors";
import { logger } from "./utils/logger";
import { globalErrorHandler } from "./middleware/error";

const app = new Hono();
const PORT = process.env.PORT || 3000;

app.use("*", httpLogger());
app.use("*", cors({ origin: "*", credentials: true }));

app.get("/", (c) => {
  logger.info("Root route hit");
  return c.text(`running on ${PORT}`);
});

app.get("/health", (c) => {
  return c.json({
    status: "ok",
    uptime: process.uptime(),
    timestamp: new Date().toISOString(),
  });
});

app.notFound((c) => c.json({ message: "Route not found" }, 404));
app.onError(globalErrorHandler);

logger.info(`Server initialized on port ${PORT}`);

export default {
  port: PORT,
  fetch: app.fetch,
};
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "src/utils/logger.ts".into(),
                content: r##"import pino from "pino";

const isProd = process.env.NODE_ENV === "production";

export const logger = pino({
  level: isProd ? "info" : "debug",
  transport: !isProd ? {
    target: "pino-pretty",
    options: {
      colorize: true,
      translateTime: "SYS:standard",
      ignore: "pid,hostname",
    },
  } : undefined,
});
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "src/middleware/error.ts".into(),
                content: r##"import type { Context } from "hono";
import { ZodError } from "zod";
import { logger } from "../utils/logger";

export const globalErrorHandler = (err: unknown, c: Context) => {
  if (err instanceof ZodError) {
    logger.warn({ err }, "Validation error");
    return c.json({ message: "Validation failed", errors: err.flatten().fieldErrors }, 400);
  }
  if (err instanceof Error) {
    logger.error({ err }, err.message);
    return c.json({ message: err.message }, 500);
  }
  logger.fatal({ err }, "Unknown error");
  return c.json({ message: "Internal Server Error" }, 500);
};
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "src/schema/user.schema.ts".into(),
                content: r##"import { z } from "zod";

export const registerSchema = z.object({
  name: z.string().min(2, "Name too short"),
  email: z.string().email("Invalid email"),
  password: z.string().min(6, "Password must be at least 6 chars"),
});

export const loginSchema = z.object({
  email: z.string().email(),
  password: z.string().min(6),
});
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "prisma/schema.prisma".into(),
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
                path: "src/utils/prisma.ts".into(),
                content: r##"import "dotenv/config";
import { PrismaPg } from '@prisma/adapter-pg'
import { PrismaClient } from "@prisma/client";
import { pg } from "pg";

const connectionString = `${process.env.DATABASE_URL}`
// Note: In a real app, you'd set up the pool and adapter properly
// This is a simplified version for scaffolding
const pool = new pg.Pool({ connectionString })
const adapter = new PrismaPg(pool)
const prisma = new PrismaClient({ adapter })

export { prisma }
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: ".env".into(),
                content: "DATABASE_URL=\"postgresql://postgres:402502@localhost:5432/admin-loginpage\"\nPORT=3000\n".into(), binary_content: None,
            },
        ],
    }
}
