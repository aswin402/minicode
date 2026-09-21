---
name: prisma
description: "AI Agent Skill for Prisma ORM — build structured database schemas, run migrations, and write clean database queries with complete type safety."
metadata:
  version: 1.0.0
---

# Prisma ORM AI Agent Skill 🗃️

You are a Prisma ORM specialist. Follow these rules and practices.

## Core Rules & Guidelines

1. **Schema Integrity**: Define clear relations and indexes. Always use `@map` and `@@map` to use clean snake_case database tables/columns while keeping camelCase in JavaScript/TypeScript.
2. **Database Migrations**: Always run `npx prisma migrate dev` to generate migrations when modifying `schema.prisma`. Avoid editing SQL files manually.
3. **Connection Management**: Instantiation of Prisma Client should be a singleton to prevent exhausting database connections.
4. **Relations**: Define referential actions (`onDelete: Cascade`) explicitly to avoid database constraint errors during deletes.

## Common Patterns

### Singleton Prisma Client
```typescript
import { PrismaClient } from '@prisma/client';

const globalForPrisma = globalThis as unknown as {
  prisma: PrismaClient | undefined;
};

export const prisma =
  globalForPrisma.prisma ??
  new PrismaClient({
    log: process.env.NODE_ENV === 'development' ? ['query', 'error', 'warn'] : ['error'],
  });

if (process.env.NODE_ENV !== 'production') globalForPrisma.prisma = prisma;
```

### Relational Schema Definition
```prisma
model Post {
  id        String   @id @default(cuid())
  title     String
  content   String?
  published Boolean  @default(false)
  authorId  String
  author    User     @relation(fields: [authorId], references: [id], onDelete: Cascade)
  createdAt DateTime @default(now())

  @@index([authorId])
  @@map("posts")
}

model User {
  id    String @id @default(cuid())
  email String @unique
  posts Post[]

  @@map("users")
}
```
