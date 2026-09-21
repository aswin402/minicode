---
name: next
description: "AI Agent Skill for Next.js — build fast, SEO-friendly fullstack applications using Next.js App Router. Focuses on Server/Client component separation, Server Actions, routing, optimization, and fetching."
metadata:
  version: 1.0.0
---

# Next.js App Router AI Agent Skill 🚀

You are a Next.js App Router specialist. Follow these rules and practices.

## Core Rules & Guidelines

1. **Server Components by Default**: All components are React Server Components (RSC) by default. Only add `'use client'` when you need interactivity (state, effects, event listeners, browser-only APIs).
2. **Data Fetching**: Fetch data directly inside Server Components using `async/await`. Avoid using route handlers or client-side fetch unless doing mutation-based UI updates.
3. **Data Mutations**: Use Server Actions for all data mutations. Ensure validation is performed inside the server action (e.g. using `zod`).
4. **Caching & Revalidation**: Use `revalidatePath` or `revalidateTag` to update cache when server actions mutate data.
5. **Dynamic / Static**: Understand static rendering vs. dynamic rendering. Use `force-dynamic` or dynamic parameters strategically.

## Common Patterns

### Server Component fetching with Suspense
```tsx
// src/app/users/page.tsx
import { Suspense } from 'react';
import { UserList, UserListSkeleton } from '@/components/user-list';

export default function UsersPage() {
  return (
    <main className="p-6">
      <h1 className="text-2xl font-bold mb-4">Users</h1>
      <Suspense fallback={<UserListSkeleton />}>
        <UserList />
      </Suspense>
    </main>
  );
}
```

### Server Action Pattern
```typescript
// src/app/actions.ts
'use server';

import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';

const schema = z.object({
  title: z.string().min(3),
});

export async function createPost(formData: FormData) {
  const validated = schema.safeParse({
    title: formData.get('title'),
  });

  if (!validated.success) {
    return { error: 'Invalid title' };
  }

  // Save to DB (e.g. Prisma)
  // await db.post.create({ data: { title: validated.data.title } });

  revalidatePath('/posts');
  redirect('/posts');
}
```
