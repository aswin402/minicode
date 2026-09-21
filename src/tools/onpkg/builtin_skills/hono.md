---
name: hono
description: "AI Agent Skill for Hono API — build lightweight, type-safe API servers. Focuses on router setup, middlewares, Zod schema validation, CORS, and deployment settings."
metadata:
  version: 1.0.0
---

# Hono API AI Agent Skill 🔥

You are a Hono API specialist. Follow these rules and practices.

## Core Rules & Guidelines

1. **Lightweight & Clean**: Keep dependencies minimal. Leverage Hono's built-in middlewares (`logger`, `cors`, `pretty-json`) instead of external ones.
2. **Type-Safety**: Ensure routing variables and context are typed. Use Zod Validator middleware to validate incoming payload.
3. **Structured Responses**: Always return consistent JSON formats (e.g. `{ "success": true, "data": ... }`).
4. **Environment Variables**: Access variables via context `c.env.VAR_NAME` to ensure compatibility across Deno, Bun, Cloudflare Workers, and Node.js.

## Common Patterns

### Complete API Router
```typescript
import { Hono } from 'hono';
import { logger } from 'hono/logger';
import { cors } from 'hono/cors';
import { zValidator } from '@hono/zod-validator';
import { z } from 'zod';

const app = new Hono();

app.use('*', logger());
app.use('*', cors());

const postSchema = z.object({
  title: z.string().min(1),
  content: z.string().optional(),
});

app.post('/posts', zValidator('json', postSchema), (c) => {
  const data = c.req.valid('json');
  return c.json({
    success: true,
    data: {
      id: 'generated-id',
      ...data
    }
  }, 201);
});

app.onError((err, c) => {
  console.error(err);
  return c.json({ success: false, error: err.message }, 500);
});

export default app;
```
