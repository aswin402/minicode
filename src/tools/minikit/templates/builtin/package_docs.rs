#![allow(clippy::all, unused)]

use crate::tools::minikit::stacks::StackFile;

pub fn react_package_docs() -> Vec<StackFile> {
    vec![
            StackFile {
                path: "minikit_docs/packages/minikit___eslint__js.md".into(),
                content: r###"# @eslint/js — minikit docs
> **Version**: 10.0.1 · **Runtime**: bun · **npm**: https://www.npmjs.com/package/@eslint/js

ESLint JavaScript language implementation
**Homepage**: https://eslint.org

---

> ✏️ Edit this file freely — it lives in ~/.minikit/docs/bun/@eslint/js.md
> Every project you add @eslint/js to will get YOUR edited version.
> To regenerate from original: `minicode skill reset @eslint/js --runtime bun`

## My Notes

<!-- Add your own notes, snippets, team conventions here -->

---

## Installation

```json
{ "dependencies": { "@eslint/js": "^10.0.1" } }
```

## Import

```typescript
import ... from '@eslint/js';
```

## Quick Start

```typescript
// Add your usage example here
```

## Links

- npm: https://www.npmjs.com/package/@eslint/js
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit___hookform__resolvers.md".into(),
                content: r###"# @hookform/resolvers

> Schema validation adapters for react-hook-form — Zod, Yup, Valibot, Joi, and more
> **Version:** 5.2.2 · **Runtime:** bun · [npm](https://www.npmjs.com/package/@hookform/resolvers)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/@hookform/resolvers.md`
> To regenerate from original: `minicode skill reset @hookform/resolvers --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add @hookform/resolvers

# Plus your chosen validation library
bun add zod        # recommended
bun add yup
bun add valibot
```

---

## Usage Pattern

```tsx
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'  // swap per library
import * as z from 'zod'

const schema = z.object({ ... })
type FormData = z.infer<typeof schema>

const { register, handleSubmit, formState: { errors } } = useForm<FormData>({
  resolver: zodResolver(schema),
})
```

---

## Zod (recommended)

```tsx
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'

const schema = z.object({
  email: z.string().email('Invalid email'),
  password: z.string().min(8, 'Min 8 characters'),
  age: z.number({ coerce: true }).int().positive(),
})

type FormData = z.infer<typeof schema>

function LoginForm() {
  const { register, handleSubmit, formState: { errors } } = useForm<FormData>({
    resolver: zodResolver(schema),
    defaultValues: { email: '', password: '' },
  })

  return (
    <form onSubmit={handleSubmit(console.log)}>
      <input {...register('email')} />
      {errors.email && <p>{errors.email.message}</p>}

      <input type="password" {...register('password')} />
      {errors.password && <p>{errors.password.message}</p>}

      <input type="number" {...register('age', { valueAsNumber: true })} />
      {errors.age && <p>{errors.age.message}</p>}

      <button type="submit">Submit</button>
    </form>
  )
}
```

---

## Yup

```tsx
import { yupResolver } from '@hookform/resolvers/yup'
import * as yup from 'yup'

const schema = yup.object({
  name: yup.string().required('Required'),
  age: yup.number().required().positive().integer(),
})

useForm({ resolver: yupResolver(schema) })
```

---

## Valibot

```tsx
import { valibotResolver } from '@hookform/resolvers/valibot'
import * as v from 'valibot'

const schema = v.object({
  username: v.pipe(v.string(), v.minLength(3, 'Min 3 chars')),
  password: v.pipe(v.string(), v.nonEmpty('Required')),
})

useForm({ resolver: valibotResolver(schema) })
```

---

## Joi

```tsx
import { joiResolver } from '@hookform/resolvers/joi'
import Joi from 'joi'

const schema = Joi.object({
  name: Joi.string().required(),
  age: Joi.number().required(),
})

useForm({ resolver: joiResolver(schema) })
```

---

## All Supported Resolvers

| Library | Import path | Type inference |
|---------|-------------|----------------|
| Zod | `/zod` | ✅ |
| Yup | `/yup` | ✅ |
| Valibot | `/valibot` | ✅ |
| Joi | `/joi` | ❌ |
| ArkType | `/arktype` | ✅ |
| TypeBox | `/typebox` | ✅ |
| class-validator | `/class-validator` | ✅ |
| Effect | `/effect-ts` | ✅ |
| io-ts | `/io-ts` | ✅ |
| Superstruct | `/superstruct` | ✅ |
| Vest | `/vest` | ❌ |
| VineJS | `/vine` | ✅ |
| Standard Schema | `/standard-schema` | ✅ |

---

## TypeScript — Input vs Output types

When a schema transforms values (e.g. coerces strings to numbers):

```tsx
useForm<z.input<typeof schema>, any, z.output<typeof schema>>({
  resolver: zodResolver(schema),
})
// input type = what the form field receives (string from <input>)
// output type = what handleSubmit receives after transformation (number)
```

---

## Resources

- [GitHub](https://github.com/react-hook-form/resolvers)
- [npm](https://www.npmjs.com/package/@hookform/resolvers)
- [react-hook-form docs](https://react-hook-form.com/docs/useform#resolver)"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit___tailwindcss__vite.md".into(),
                content: r###"# @tailwindcss/vite — minikit docs
> **Version**: 4.2.2 · **Runtime**: bun · **npm**: https://www.npmjs.com/package/@tailwindcss/vite

A utility-first CSS framework for rapidly building custom user interfaces.
**Homepage**: https://tailwindcss.com

---

> ✏️ Edit this file freely — it lives in ~/.minikit/docs/bun/@tailwindcss/vite.md
> Every project you add @tailwindcss/vite to will get YOUR edited version.
> To regenerate from original: `minicode skill reset @tailwindcss/vite --runtime bun`

## My Notes

<!-- Add your own notes, snippets, team conventions here -->

---

## Installation

```json
{ "dependencies": { "@tailwindcss/vite": "^4.2.2" } }
```

## Import

```typescript
import ... from '@tailwindcss/vite';
```

## Quick Start

```typescript
// Add your usage example here
```

## Links

- npm: https://www.npmjs.com/package/@tailwindcss/vite
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit___tanstack__react-query.md".into(),
                content: r####"# @tanstack/react-query

> Server state management — fetching, caching, syncing async data in React
> **Version:** 5.95.0 · **Runtime:** bun · [npm](https://www.npmjs.com/package/@tanstack/react-query) · [Docs](https://tanstack.com/query)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/@tanstack/react-query.md`
> To regenerate from original: `minicode skill reset @tanstack/react-query --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add @tanstack/react-query
bun add -d @tanstack/react-query-devtools
```

## Import

```ts
import { useQuery, useMutation, useQueryClient, QueryClient, QueryClientProvider } from '@tanstack/react-query'
```

> **Mental model:** TanStack Query owns **server state** (API data). Use `useState`/`zustand` for **client state** (modals, UI toggles).

---

## Setup

```tsx
// main.tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 60 * 1000,     // 1 minute — data is fresh for this long
      retry: 2,                 // retry failed requests twice
    },
  },
})

createRoot(document.getElementById('root')!).render(
  <QueryClientProvider client={queryClient}>
    <App />
    <ReactQueryDevtools initialIsOpen={false} />
  </QueryClientProvider>
)
```

---

## `useQuery` — fetch & cache data

```tsx
const { data, isPending, isError, error, isFetching, refetch } = useQuery({
  queryKey: ['users'],           // cache key — array, treated like a dependency array
  queryFn: () => fetch('/api/users').then(r => r.json()),
})

// With parameter
const { data: user } = useQuery({
  queryKey: ['user', userId],    // changes when userId changes → auto refetch
  queryFn: () => api.getUser(userId),
  enabled: !!userId,             // only run when userId is truthy
})
```

### `useQuery` return values

```ts
data          // T | undefined — the resolved data
isPending     // true while loading for the first time
isFetching    // true whenever a fetch is in flight (including background)
isError       // true if the last fetch failed
error         // Error | null
isSuccess     // true if data is available
dataUpdatedAt // timestamp of last successful fetch
refetch()     // manually trigger a refetch
```

### `useQuery` options

```ts
useQuery({
  queryKey: ['todos'],
  queryFn: fetchTodos,
  staleTime: 1000 * 60 * 5,    // 5 min — don't refetch if data is fresh
  gcTime: 1000 * 60 * 10,      // 10 min — keep in cache after unmount (was cacheTime in v4)
  refetchOnWindowFocus: true,   // refetch when tab regains focus (default: true)
  refetchOnMount: true,         // refetch when component mounts (default: true)
  refetchInterval: 5000,        // poll every 5 seconds (false to disable)
  retry: 3,                     // number of retries on failure
  enabled: true,                // conditionally run the query
  select: (data) => data.items, // transform data — only rerenders if result changes
  placeholderData: [],          // shown while loading (not cached)
  initialData: cachedData,      // treated as real data, skips loading state
})
```

---

## `useMutation` — create, update, delete

```tsx
const mutation = useMutation({
  mutationFn: (newUser: CreateUserDTO) =>
    fetch('/api/users', {
      method: 'POST',
      body: JSON.stringify(newUser),
      headers: { 'Content-Type': 'application/json' },
    }).then(r => r.json()),

  onSuccess: (data) => {
    queryClient.invalidateQueries({ queryKey: ['users'] }) // refetch users list
  },
  onError: (error) => {
    toast.error(error.message)
  },
  onSettled: () => {
    // runs after success or error
  },
})

// Trigger it
mutation.mutate({ name: 'Jane', email: 'jane@example.com' })

// Or async/await
await mutation.mutateAsync({ name: 'Jane' })
```

### `useMutation` states

```ts
mutation.isPending   // true while running
mutation.isSuccess   // true after success
mutation.isError     // true after failure
mutation.isIdle      // true before first call
mutation.data        // T — result data
mutation.error       // Error | null
mutation.variables   // the variables passed to mutate()
mutation.reset()     // reset to idle state
```

---

## `useQueryClient` — imperative cache control

```tsx
const queryClient = useQueryClient()

// Invalidate — mark stale → triggers refetch on next render
queryClient.invalidateQueries({ queryKey: ['users'] })

// Invalidate all queries starting with 'user'
queryClient.invalidateQueries({ queryKey: ['user'] })

// Manually set cache data (e.g. after create/update)
queryClient.setQueryData(['user', id], updatedUser)

// Update part of cached data
queryClient.setQueryData(['users'], (old: User[]) =>
  old.map(u => u.id === id ? { ...u, name: 'Jane' } : u)
)

// Prefetch — load data before navigation
await queryClient.prefetchQuery({
  queryKey: ['user', id],
  queryFn: () => api.getUser(id),
})

// Read cache without subscribing
const users = queryClient.getQueryData<User[]>(['users'])

// Remove from cache
queryClient.removeQueries({ queryKey: ['users'] })

// Cancel in-flight requests
await queryClient.cancelQueries({ queryKey: ['users'] })
```

---

## Query Keys — best practice

```ts
// Treat like a dependency array — include everything the queryFn uses
['users']                           // all users
['user', userId]                    // specific user
['user', userId, 'posts']           // user's posts
['users', { status: 'active' }]    // filtered
['search', { q: 'flutter', page: 1 }]
```

---

## Parallel & Dependent Queries

```tsx
// Parallel — both run at the same time
const usersQuery = useQuery({ queryKey: ['users'], queryFn: fetchUsers })
const postsQuery = useQuery({ queryKey: ['posts'], queryFn: fetchPosts })

// Dependent — waits for userId before running
const { data: user } = useQuery({ queryKey: ['user', id], queryFn: () => fetchUser(id) })
const { data: orders } = useQuery({
  queryKey: ['orders', user?.id],
  queryFn: () => fetchOrders(user!.id),
  enabled: !!user,   // only runs after user is loaded
})
```

---

## Infinite / Paginated Queries

```tsx
import { useInfiniteQuery } from '@tanstack/react-query'

const { data, fetchNextPage, hasNextPage, isFetchingNextPage } = useInfiniteQuery({
  queryKey: ['posts'],
  queryFn: ({ pageParam }) => fetchPosts({ page: pageParam }),
  initialPageParam: 1,
  getNextPageParam: (lastPage, pages) =>
    lastPage.hasMore ? pages.length + 1 : undefined,
})

// data.pages is an array of pages — flatten to render
const allPosts = data?.pages.flatMap(p => p.items) ?? []
```

---

## Optimistic Updates

```tsx
const queryClient = useQueryClient()

const mutation = useMutation({
  mutationFn: updateUser,
  onMutate: async (updated) => {
    await queryClient.cancelQueries({ queryKey: ['user', updated.id] })
    const previous = queryClient.getQueryData<User>(['user', updated.id])
    queryClient.setQueryData(['user', updated.id], updated)  // update cache immediately
    return { previous }  // return for rollback
  },
  onError: (err, updated, context) => {
    // Roll back on failure
    queryClient.setQueryData(['user', updated.id], context?.previous)
  },
  onSettled: (_, __, updated) => {
    queryClient.invalidateQueries({ queryKey: ['user', updated.id] })
  },
})
```

---

## Custom Hook Pattern (recommended)

```ts
// hooks/useUsers.ts
export function useUsers() {
  return useQuery({ queryKey: ['users'], queryFn: api.getUsers })
}

export function useUser(id: string) {
  return useQuery({ queryKey: ['user', id], queryFn: () => api.getUser(id), enabled: !!id })
}

export function useCreateUser() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: api.createUser,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['users'] }),
  })
}
```

---

## Resources

- [TanStack Query Docs](https://tanstack.com/query/latest)
- [GitHub](https://github.com/TanStack/query)
- [DevTools](https://tanstack.com/query/latest/docs/framework/react/devtools)
- [v4 → v5 Migration Guide](https://tanstack.com/query/v5/docs/framework/react/guides/migrating-to-v5)"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit___types__node.md".into(),
                content: r####"# @types/node — minikit docs
> **Version**: 25.5.0 · **Runtime**: bun · **npm**: https://www.npmjs.com/package/@types/node

TypeScript definitions for node
**Homepage**: https://github.com/DefinitelyTyped/DefinitelyTyped/tree/master/types/node

---

> ✏️ Edit this file freely — it lives in ~/.minikit/docs/bun/@types/node.md
> Every project you add @types/node to will get YOUR edited version.
> To regenerate from original: `minicode skill reset @types/node --runtime bun`

## My Notes

<!-- Add your own notes, snippets, team conventions here -->

---

# Installation
> `npm install --save @types/node`

# Summary
This package contains type definitions for node (https://nodejs.org/).

# Details
Files were exported from https://github.com/DefinitelyTyped/DefinitelyTyped/tree/master/types/node.

### Additional Details
 * Last updated: Thu, 12 Mar 2026 15:47:58 GMT
 * Dependencies: [undici-types](https://npmjs.com/package/undici-types)

# Credits
These definitions were written by [Microsoft TypeScript](https://github.com/Microsoft), [Alberto Schiabel](https://github.com/jkomyno), [Andrew Makarov](https://github.com/r3nya), [Benjamin Toueg](https://github.com/btoueg), [David Junger](https://github.com/touffy), [Mohsen Azimi](https://github.com/mohsen1), [Nikita Galkin](https://github.com/galkin), [Sebastian Silbermann](https://github.com/eps1lon), [Wilco Bakker](https://github.com/WilcoBakker), [Marcin Kopacz](https://github.com/chyzwar), [Trivikram Kamat](https://github.com/trivikr), [Junxiao Shi](https://github.com/yoursunny), [Ilia Baryshnikov](https://github.com/qwelias), [ExE Boss](https://github.com/ExE-Boss), [Piotr Błażejewicz](https://github.com/peterblazejewicz), [Anna Henningsen](https://github.com/addaleax), [Victor Perin](https://github.com/victorperin), [NodeJS Contributors](https://github.com/NodeJS), [Linus Unnebäck](https://github.com/LinusU), [wafuwafu13](https://github.com/wafuwafu13), [Matteo Collina](https://github.com/mcollina), [Dmitry Semigradsky](https://github.com/Semigradsky), [René](https://github.com/Renegade334), and [Yagiz Nizipli](https://github.com/anonrig).
"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_axios.md".into(),
                content: r####"# axios

> Promise-based HTTP client for the browser and Node.js
> **Version:** 1.9.0 · **Runtime:** bun · [npm](https://www.npmjs.com/package/axios) · [Docs](https://axios-http.com/docs/intro)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/axios.md`
> Every project you add axios to will use YOUR version of this file.
> To regenerate from original: `minicode skill reset axios --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add axios
```

## Import

```js
import axios from 'axios'

// Named imports
import axios, { isCancel, AxiosError } from 'axios'

// CommonJS
const axios = require('axios')
```

---

## Usage

### GET

```js
// Simple
const res = await axios.get('/users?id=1')
console.log(res.data)

// With params
const res = await axios.get('/users', { params: { id: 1 } })

// Async/await with error handling
try {
  const res = await axios.get('/users/1')
  console.log(res.data)
} catch (err) {
  console.error(err)
}
```

### POST

```js
const res = await axios.post('/users', {
  name: 'John',
  email: 'john@example.com'
})
```

### PUT / PATCH / DELETE

```js
await axios.put('/users/1', { name: 'Updated' })
await axios.patch('/users/1', { name: 'Patched' })
await axios.delete('/users/1')
```

### Generic config-based call

```js
await axios({
  method: 'post',
  url: '/users',
  data: { name: 'John' }
})
```

### Concurrent requests

```js
const [users, posts] = await Promise.all([
  axios.get('/users'),
  axios.get('/posts')
])
```

---

## Creating an Instance

Use instances to avoid repeating `baseURL`, headers, or timeouts across requests.

```js
const api = axios.create({
  baseURL: 'https://api.example.com',
  timeout: 5000,
  headers: { 'Authorization': `Bearer ${TOKEN}` }
})

// Use it like the global axios
const res = await api.get('/users')
```

---

## Request Config

Only `url` is required. All other fields are optional.

```js
{
  url: '/users',
  method: 'get',                          // default
  baseURL: 'https://api.example.com',
  headers: { 'X-Custom': 'value' },
  params: { id: 1 },                      // appended to URL as query string
  data: { name: 'John' },                 // request body (POST/PUT/PATCH/DELETE)
  timeout: 5000,                          // ms — 0 means no timeout (default)
  responseType: 'json',                   // 'arraybuffer' | 'blob' | 'document' | 'json' | 'text' | 'stream'
  withCredentials: false,                 // send cookies with cross-site requests
  maxRedirects: 5,
  auth: {
    username: 'user',
    password: 'pass'
  },
  validateStatus: (status) => status < 500  // customize which status codes reject
}
```

---

## Response Schema

```js
const res = await axios.get('/users/1')

res.data        // parsed response body
res.status      // HTTP status code (e.g. 200)
res.statusText  // HTTP status message (e.g. "OK")
res.headers     // response headers
res.config      // the config used for the request
res.request     // the underlying request object
```

---

## Config Defaults

Set defaults applied to every request globally or per-instance.

```js
// Global defaults
axios.defaults.baseURL = 'https://api.example.com'
axios.defaults.headers.common['Authorization'] = `Bearer ${TOKEN}`

// Instance defaults
const api = axios.create({ baseURL: 'https://api.example.com' })
api.defaults.headers.common['Authorization'] = `Bearer ${TOKEN}`
```

**Precedence (highest to lowest):**
per-request config → instance defaults → global defaults

---

## Interceptors

Run logic before every request is sent or after every response is received.

```js
const api = axios.create()

// Request interceptor — runs before the request
api.interceptors.request.use(
  (config) => {
    config.headers['X-Request-Time'] = Date.now()
    return config
  },
  (error) => Promise.reject(error)
)

// Response interceptor — runs after the response
api.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      // handle token refresh or redirect
    }
    return Promise.reject(error)
  }
)
```

**Remove an interceptor:**

```js
const id = api.interceptors.request.use(fn)
api.interceptors.request.eject(id)
```

**Clear all interceptors:**

```js
api.interceptors.request.clear()
api.interceptors.response.clear()
```

> **Note:** Request interceptors run in **reverse order** (LIFO).
> Response interceptors run in **insertion order** (FIFO).

---

## Error Handling

```js
try {
  await axios.get('/users/1')
} catch (err) {
  if (err.response) {
    // Server responded with a non-2xx status
    console.log(err.response.status)
    console.log(err.response.data)
  } else if (err.request) {
    // Request was made but no response received
    console.log('No response received')
  } else {
    // Error setting up the request
    console.log(err.message)
  }
}
```

**Common error codes:**

| Code | Meaning |
|------|---------|
| `ERR_NETWORK` | Network failure or CORS issue |
| `ECONNABORTED` | Request timed out or was aborted |
| `ETIMEDOUT` | Timeout (when `clarifyTimeoutError: true`) |
| `ERR_CANCELED` | Manually canceled via AbortController |
| `ERR_BAD_REQUEST` | 4xx response |
| `ERR_BAD_RESPONSE` | 5xx or unparseable response |
| `ERR_INVALID_URL` | Malformed URL |

---

## Timeouts

```js
try {
  const res = await axios.get('/slow-endpoint', { timeout: 5000 })
} catch (err) {
  if (axios.isAxiosError(err) && err.code === 'ECONNABORTED') {
    console.error('Request timed out')
  }
}
```

---

## Cancellation

Use the native `AbortController` (recommended):

```js
const controller = new AbortController()

axios.get('/users', { signal: controller.signal })
  .then(res => console.log(res.data))
  .catch(err => {
    if (axios.isCancel(err)) console.log('Canceled')
  })

// Cancel the request
controller.abort()
```

---

## Form Data

### URL-encoded (`application/x-www-form-urlencoded`)

```js
const params = new URLSearchParams({ foo: 'bar', baz: '123' })
await axios.post('/submit', params)
```

**Auto-serialization** — set the content-type header and axios handles the rest:

```js
await axios.post('/submit', { foo: 'bar' }, {
  headers: { 'content-type': 'application/x-www-form-urlencoded' }
})
```

### Multipart (`multipart/form-data`)

```js
const form = new FormData()
form.append('name', 'John')
form.append('file', fileInput.files[0])
await axios.post('/upload', form)
```

**Auto-serialization:**

```js
await axios.post('/upload', { name: 'John' }, {
  headers: { 'Content-Type': 'multipart/form-data' }
})
```

**Shorthand methods:** `axios.postForm`, `axios.putForm`, `axios.patchForm`

```js
await axios.postForm('/upload', { file: fileInput.files[0] })
```

---

## Upload / Download Progress

```js
await axios.post('/upload', data, {
  onUploadProgress: ({ progress, rate, estimated }) => {
    console.log(`${(progress * 100).toFixed(1)}% — ${(rate / 1024).toFixed(1)} KB/s`)
  },
  onDownloadProgress: ({ progress }) => {
    console.log(`Download: ${(progress * 100).toFixed(1)}%`)
  }
})
```

> Progress events are throttled to **3 times per second**.

---

## Rate Limiting *(Node.js only)*

```js
await axios.post('/upload', buffer, {
  maxRate: [100 * 1024, 100 * 1024]  // [uploadLimit, downloadLimit] in bytes/s
})
```

---

## Headers (`AxiosHeaders`)

```js
// Inside a request interceptor
config.headers.set('Authorization', `Bearer ${token}`)
config.headers.setContentType('application/json')

// Disable a header
config.headers.set('User-Agent', false)
```

---

## TypeScript

```ts
import axios, { AxiosResponse, AxiosError } from 'axios'

interface User {
  id: number
  name: string
}

const res: AxiosResponse<User> = await axios.get<User>('/users/1')
console.log(res.data.name)
```

---

## Proxy *(Node.js only)*

```js
await axios.get('/data', {
  proxy: {
    protocol: 'https',
    host: '127.0.0.1',
    port: 9000,
    auth: { username: 'user', password: 'pass' }
  }
})
```

---

## Resources

- [Official Docs](https://axios-http.com/docs/intro)
- [GitHub](https://github.com/axios/axios)
- [Changelog](https://github.com/axios/axios/blob/main/CHANGELOG.md)"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_eslint-plugin-react-hooks.md".into(),
                content: r###"# eslint-plugin-react-hooks — minikit docs
> **Version**: 7.0.1 · **Runtime**: bun · **npm**: https://www.npmjs.com/package/eslint-plugin-react-hooks

ESLint rules for React Hooks
**Homepage**: https://react.dev/

---

> ✏️ Edit this file freely — it lives in ~/.minikit/docs/bun/eslint-plugin-react-hooks.md
> Every project you add eslint-plugin-react-hooks to will get YOUR edited version.
> To regenerate from original: `minicode skill reset eslint-plugin-react-hooks --runtime bun`

## My Notes

<!-- Add your own notes, snippets, team conventions here -->

---

## Installation

```json
{ "dependencies": { "eslint-plugin-react-hooks": "^7.0.1" } }
```

## Import

```typescript
import ... from 'eslint-plugin-react-hooks';
```

## Quick Start & Critical v7 Rules

- **`react-hooks/set-state-in-effect` (Error in v7)**: Never call `setState` synchronously inside the top-level body of `useEffect` (e.g., `useEffect(() => { if (reducedMotion) { setVisible(true); return; } ... }, [])`).
  - **Fix**: Initialize state lazily in `useState(() => prefersReducedMotion())` or derive state during render, and only call `setState` inside asynchronous callbacks (`setTimeout`, `setInterval`, `requestAnimationFrame`, `IntersectionObserver`, `fetch().then(...)`, or DOM event listeners).

```tsx
// ✅ Pass: lazy initial state + setState only inside observer/timer callback
const [visible, setVisible] = useState(() =>
  typeof window !== 'undefined' &&
  window.matchMedia('(prefers-reduced-motion: reduce)').matches,
);

useEffect(() => {
  if (visible) return;
  const io = new IntersectionObserver(([entry]) => {
    if (entry?.isIntersecting) {
      setVisible(true);
      io.disconnect();
    }
  });
  if (ref.current) io.observe(ref.current);
  return () => io.disconnect();
}, [visible]);
```

## Links

- npm: https://www.npmjs.com/package/eslint-plugin-react-hooks
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_eslint-plugin-react-refresh.md".into(),
                content: r####"# eslint-plugin-react-refresh — minikit docs
> **Version**: 0.5.2 · **Runtime**: bun · **npm**: https://www.npmjs.com/package/eslint-plugin-react-refresh

Validate that your components can safely be updated with Fast Refresh
**Homepage**: https://github.com/ArnaudBarre/eslint-plugin-react-refresh#readme

---

> ✏️ Edit this file freely — it lives in ~/.minikit/docs/bun/eslint-plugin-react-refresh.md
> Every project you add eslint-plugin-react-refresh to will get YOUR edited version.
> To regenerate from original: `minicode skill reset eslint-plugin-react-refresh --runtime bun`

## My Notes

<!-- Add your own notes, snippets, team conventions here -->

---

# eslint-plugin-react-refresh [![npm](https://img.shields.io/npm/v/eslint-plugin-react-refresh)](https://www.npmjs.com/package/eslint-plugin-react-refresh)

Validate that your components can safely be updated with Fast Refresh.

## Explainer

"Fast Refresh", also known as "hot reloading", is a feature in many modern bundlers.
If you update some React component(s) on disk, then the bundler will know to update only the impacted parts of your page -- without a full page reload.

`eslint-plugin-react-refresh` enforces that your components are structured in a way that integrations such as [react-refresh](https://www.npmjs.com/package/react-refresh) expect.

### Limitations

⚠️ To avoid false positives, by default this plugin is only applied on `tsx` & `jsx` files. See [Options](#options) to run on JS files. ⚠️

The plugin relies on naming conventions (i.e. use PascalCase for components, camelCase for util functions). This is why there are some limitations:

- `export *` are not supported and will be reported as an error
- Anonymous function are not supported (i.e `export default function() {}`)
- Class components are not supported
- All-uppercase function export is considered an error when not using direct named export (ex `const CMS = () => <></>; export { CMS }`)

## Installation

```sh
npm i -D eslint-plugin-react-refresh
```

## Usage

This plugin provides a single rule, `react-refresh/only-export-components`. There are multiple ways to enable it.

### Recommended config

```js
import { defineConfig } from "eslint/config";
import { reactRefresh } from "eslint-plugin-react-refresh";

export default defineConfig(
  /* Main config */
  reactRefresh.configs.recommended(),
);
```

### Vite config

This enables the `allowConstantExport` option which is supported by Vite React plugins.

```js
import { defineConfig } from "eslint/config";
import { reactRefresh } from "eslint-plugin-react-refresh";

export default defineConfig(
  /* Main config */
  reactRefresh.configs.vite(),
);
```

### Next config

This allows exports like `fetchCache` and `revalidate` which are used in Page or Layout components and don't trigger a full page reload.

```js
import { defineConfig } from "eslint/config";
import { reactRefresh } from "eslint-plugin-react-refresh";

export default defineConfig(
  /* Main config */
  reactRefresh.configs.next(),
);
```

### Without config

```js
import { defineConfig } from "eslint/config";
import { reactRefresh } from "eslint-plugin-react-refresh";

export default defineConfig({
  // in main config for TSX/JSX source files
  plugins: {
    "react-refresh": reactRefresh.plugin,
  },
  rules: {
    "react-refresh/only-export-components": "error",
  },
});
```

## Examples

These examples are from enabling `react-refresh/only-exports-components`.

### Fail

```jsx
export const foo = () => {};
export const Bar = () => <></>;
```

```jsx
export default function () {}
export default compose()(MainComponent)
```

```jsx
export * from "./foo";
```

```jsx
const Tab = () => {};
export const tabs = [<Tab />, <Tab />];
```

```jsx
const App = () => {};
createRoot(document.getElementById("root")).render(<App />);
```

### Pass

```jsx
export default function Foo() {
  return <></>;
}
```

```jsx
const foo = () => {};
export const Bar = () => <></>;
```

```jsx
import { App } from "./App";
createRoot(document.getElementById("root")).render(<App />);
```

## Options

These options are all present on `react-refresh/only-exports-components`.

```ts
interface Options {
  extraHOCs?: string[];
  allowExportNames?: string[];
  allowConstantExport?: boolean;
  checkJS?: boolean;
}

const defaultOptions: Options = {
  extraHOCs: [],
  allowExportNames: [],
  allowConstantExport: false,
  checkJS: false,
};
```

### extraHOCs <small>(v0.5.0)</small>

If you're exporting components wrapped in non built-in React HOC (memo, forwardRef, lazy), you can use this option to avoid false positives.

```json
{
  "react-refresh/only-export-components": [
    "error",
    { "extraHOCs": ["observer", "withAuth"] }
  ]
}
```

### allowExportNames <small>(v0.4.4)</small>

> Default: `[]`

If you use a framework that handles HMR of some specific exports, you can use this option to avoid warning for them.

Example for [Remix](https://remix.run/docs/en/main/discussion/hot-module-replacement#supported-exports):

```json
{
  "react-refresh/only-export-components": [
    "error",
    { "allowExportNames": ["meta", "links", "headers", "loader", "action"] }
  ]
}
```

### allowConstantExport <small>(v0.4.0)</small>

> Default: `false` (`true` in `vite` config)

Don't warn when a constant (string, number, boolean, templateLiteral) is exported aside one or more components.

This should be enabled if the fast refresh implementation correctly handles this case (HMR when the constant doesn't change, propagate update to importers when the constant changes.). Vite supports it, PR welcome if you notice other integrations works well.

```json
{
  "react-refresh/only-export-components": [
    "error",
    { "allowConstantExport": true }
  ]
}
```

Enabling this option allows code such as the following:

```jsx
export const CONSTANT = 3;
export const Foo = () => <></>;
```

### checkJS <small>(v0.3.3)</small>

> Default: `false`

If you're using JSX inside `.js` files (which I don't recommend because it forces you to configure every tool you use to switch the parser), you can still use the plugin by enabling this option. To reduce the number of false positive, only files importing `react` are checked.

```json
{
  "react-refresh/only-export-components": ["error", { "checkJS": true }]
}
```
"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_eslint.md".into(),
                content: r####"# eslint — minikit docs
> **Version**: 10.1.0 · **Runtime**: bun · **npm**: https://www.npmjs.com/package/eslint

An AST-based pattern checker for JavaScript.
**Homepage**: https://eslint.org

---

> ✏️ Edit this file freely — it lives in ~/.minikit/docs/bun/eslint.md
> Every project you add eslint to will get YOUR edited version.
> To regenerate from original: `minicode skill reset eslint --runtime bun`

## My Notes

<!-- Add your own notes, snippets, team conventions here -->

---

[![npm version](https://img.shields.io/npm/v/eslint.svg)](https://www.npmjs.com/package/eslint)
[![Downloads](https://img.shields.io/npm/dm/eslint.svg)](https://www.npmjs.com/package/eslint)
[![Build Status](https://github.com/eslint/eslint/workflows/CI/badge.svg)](https://github.com/eslint/eslint/actions)
<br>
[![Open Collective Backers](https://img.shields.io/opencollective/backers/eslint)](https://opencollective.com/eslint)
[![Open Collective Sponsors](https://img.shields.io/opencollective/sponsors/eslint)](https://opencollective.com/eslint)

# ESLint

[Website](https://eslint.org) |
[Configure ESLint](https://eslint.org/docs/latest/use/configure) |
[Rules](https://eslint.org/docs/rules/) |
[Contribute to ESLint](https://eslint.org/docs/latest/contribute) |
[Report Bugs](https://eslint.org/docs/latest/contribute/report-bugs) |
[Code of Conduct](https://eslint.org/conduct) |
[X](https://x.com/geteslint) |
[Discord](https://eslint.org/chat) |
[Mastodon](https://fosstodon.org/@eslint) |
[Bluesky](https://bsky.app/profile/eslint.org)

ESLint is a tool for identifying and reporting on patterns found in ECMAScript/JavaScript code. In many ways, it is similar to JSLint and JSHint with a few exceptions:

- ESLint uses [Espree](https://github.com/eslint/js/tree/main/packages/espree) for JavaScript parsing.
- ESLint uses an AST to evaluate patterns in code.
- ESLint is completely pluggable, every single rule is a plugin and you can add more at runtime.

## Table of Contents

1. [Installation and Usage](#installation-and-usage)
1. [Configuration](#configuration)
1. [Version Support](#version-support)
1. [Code of Conduct](#code-of-conduct)
1. [Filing Issues](#filing-issues)
1. [Frequently Asked Questions](#frequently-asked-questions)
1. [Releases](#releases)
1. [Security Policy](#security-policy)
1. [Semantic Versioning Policy](#semantic-versioning-policy)
1. [ESM Dependencies](#esm-dependencies)
1. [License](#license)
1. [Team](#team)
1. [Sponsors](#sponsors)
1. [Technology Sponsors](#technology-sponsors) <!-- markdownlint-disable-line MD051 -->

## Installation and Usage

### Prerequisites

To use ESLint, you must have [Node.js](https://nodejs.org/) (`^20.19.0`, `^22.13.0`, or `>=24`) installed and built with SSL support. (If you are using an official Node.js distribution, SSL is always built in.)

If you use ESLint's TypeScript type definitions, TypeScript 5.3 or later is required.

### npm Installation

You can install and configure ESLint using this command:

```shell
npm init @eslint/config@latest
```

After that, you can run ESLint on any file or directory like this:

```shell
npx eslint yourfile.js
```

### pnpm Installation

To use ESLint with pnpm, we recommend setting up a `.npmrc` file with at least the following settings:

```text
auto-install-peers=true
node-linker=hoisted
```

This ensures that pnpm installs dependencies in a way that is more compatible with npm and is less likely to produce errors.

## Configuration

You can configure rules in your `eslint.config.js` files as in this example:

```js
import { defineConfig } from "eslint/config";

export default defineConfig([
	{
		files: ["**/*.js", "**/*.cjs", "**/*.mjs"],
		rules: {
			"prefer-const": "warn",
			"no-constant-binary-expression": "error",
		},
	},
]);
```

The names `"prefer-const"` and `"no-constant-binary-expression"` are the names of [rules](https://eslint.org/docs/rules) in ESLint. The first value is the error level of the rule and can be one of these values:

- `"off"` or `0` - turn the rule off
- `"warn"` or `1` - turn the rule on as a warning (doesn't affect exit code)
- `"error"` or `2` - turn the rule on as an error (exit code will be 1)

The three error levels allow you fine-grained control over how ESLint applies rules (for more configuration options and details, see the [configuration docs](https://eslint.org/docs/latest/use/configure)).

## Version Support

The ESLint team provides ongoing support for the current version and six months of limited support for the previous version. Limited support includes critical bug fixes, security issues, and compatibility issues only.

ESLint offers commercial support for both current and previous versions through our partners, [Tidelift][tidelift] and [HeroDevs][herodevs].

See [Version Support](https://eslint.org/version-support) for more details.

## Code of Conduct

ESLint adheres to the [OpenJS Foundation Code of Conduct](https://eslint.org/conduct).

## Filing Issues

Before filing an issue, please be sure to read the guidelines for what you're reporting:

- [Bug Report](https://eslint.org/docs/latest/contribute/report-bugs)
- [Propose a New Rule](https://eslint.org/docs/latest/contribute/propose-new-rule)
- [Proposing a Rule Change](https://eslint.org/docs/latest/contribute/propose-rule-change)
- [Request a Change](https://eslint.org/docs/latest/contribute/request-change)

## Frequently Asked Questions

### Does ESLint support JSX?

Yes, ESLint natively supports parsing JSX syntax (this must be enabled in [configuration](https://eslint.org/docs/latest/use/configure)). Please note that supporting JSX syntax _is not_ the same as supporting React. React applies specific semantics to JSX syntax that ESLint doesn't recognize. We recommend using [eslint-plugin-react](https://www.npmjs.com/package/eslint-plugin-react) if you are using React and want React semantics.

### Does Prettier replace ESLint?

No, ESLint and Prettier have different jobs: ESLint is a linter (looking for problematic patterns) and Prettier is a code formatter. Using both tools is common, refer to [Prettier's documentation](https://prettier.io/docs/en/install#eslint-and-other-linters) to learn how to configure them to work well with each other.

### What ECMAScript versions does ESLint support?

ESLint has full support for ECMAScript 3, 5, and every year from 2015 up until the most recent stage 4 specification (the default). You can set your desired ECMAScript syntax and other settings (like global variables) through [configuration](https://eslint.org/docs/latest/use/configure).

### What about experimental features?

ESLint's parser only officially supports the latest final ECMAScript standard. We will make changes to core rules in order to avoid crashes on stage 3 ECMAScript syntax proposals (as long as they are implemented using the correct experimental ESTree syntax). We may make changes to core rules to better work with language extensions (such as JSX, Flow, and TypeScript) on a case-by-case basis.

In other cases (including if rules need to warn on more or fewer cases due to new syntax, rather than just not crashing), we recommend you use other parsers and/or rule plugins. If you are using Babel, you can use [@babel/eslint-parser](https://www.npmjs.com/package/@babel/eslint-parser) and [@babel/eslint-plugin](https://www.npmjs.com/package/@babel/eslint-plugin) to use any option available in Babel.

Once a language feature has been adopted into the ECMAScript standard (stage 4 according to the [TC39 process](https://tc39.github.io/process-document/)), we will accept issues and pull requests related to the new feature, subject to our [contributing guidelines](https://eslint.org/docs/latest/contribute). Until then, please use the appropriate parser and plugin(s) for your experimental feature.

### Which Node.js versions does ESLint support?

ESLint updates the supported Node.js versions with each major release of ESLint. At that time, ESLint's supported Node.js versions are updated to be:

1. The most recent maintenance release of Node.js
1. The lowest minor version of the Node.js LTS release that includes the features the ESLint team wants to use.
1. The Node.js Current release

ESLint is also expected to work with Node.js versions released after the Node.js Current release.

Refer to the [Quick Start Guide](https://eslint.org/docs/latest/use/getting-started#prerequisites) for the officially supported Node.js versions for a given ESLint release.

### Where to ask for help?

Open a [discussion](https://github.com/eslint/eslint/discussions) or stop by our [Discord server](https://eslint.org/chat).

### Why doesn't ESLint lock dependency versions?

Lock files like `package-lock.json` are helpful for deployed applications. They ensure that dependencies are consistent between environments and across deployments.

Packages like `eslint` that get published to the npm registry do not include lock files. `npm install eslint` as a user will respect version constraints in ESLint's `package.json`. ESLint and its dependencies will be included in the user's lock file if one exists, but ESLint's own lock file would not be used.

We intentionally don't lock dependency versions so that we have the latest compatible dependency versions in development and CI that our users get when installing ESLint in a project.

The Twilio blog has a [deeper dive](https://www.twilio.com/blog/lockfiles-nodejs) to learn more.

## Releases

We have scheduled releases every two weeks on Friday or Saturday. You can follow a [release issue](https://github.com/eslint/eslint/issues?q=is%3Aopen+is%3Aissue+label%3Arelease) for updates about the scheduling of any particular release.

## Security Policy

ESLint takes security seriously. We work hard to ensure that ESLint is safe for everyone and that security issues are addressed quickly and responsibly. Read the full [security policy](https://github.com/eslint/.github/blob/master/SECURITY.md).

## Semantic Versioning Policy

ESLint follows [semantic versioning](https://semver.org). However, due to the nature of ESLint as a code quality tool, it's not always clear when a minor or major version bump occurs. To help clarify this for everyone, we've defined the following semantic versioning policy for ESLint:

- Patch release (intended to not break your lint build)
    - A bug fix in a rule that results in ESLint reporting fewer linting errors.
    - A bug fix to the CLI or core (including formatters).
    - Improvements to documentation.
    - Non-user-facing changes such as refactoring code, adding, deleting, or modifying tests, and increasing test coverage.
    - Re-releasing after a failed release (i.e., publishing a release that doesn't work for anyone).
- Minor release (might break your lint build)
    - A bug fix in a rule that results in ESLint reporting more linting errors.
    - A new rule is created.
    - A new option to an existing rule that does not result in ESLint reporting more linting errors by default.
    - A new addition to an existing rule to support a newly-added language feature (within the last 12 months) that will result in ESLint reporting more linting errors by default.
    - An existing rule is deprecated.
    - A new CLI capability is created.
    - New capabilities to the public API are added (new classes, new methods, new arguments to existing methods, etc.).
    - A new formatter is created.
    - `eslint:recommended` is updated and will result in strictly fewer linting errors (e.g., rule removals).
- Major release (likely to break your lint build)
    - `eslint:recommended` is updated and may result in new linting errors (e.g., rule additions, most rule option updates).
    - A new option to an existing rule that results in ESLint reporting more linting errors by default.
    - An existing formatter is removed.
    - Part of the public API is removed or changed in an incompatible way. The public API includes:
        - Rule schemas
        - Configuration schema
        - Command-line options
        - Node.js API
        - Rule, formatter, parser, plugin APIs

According to our policy, any minor update may report more linting errors than the previous release (ex: from a bug fix). As such, we recommend using the tilde (`~`) in `package.json` e.g. `"eslint": "~3.1.0"` to guarantee the results of your builds.

## ESM Dependencies

Since ESLint is a CommonJS package, there are restrictions on which ESM-only packages can be used as dependencies.

Packages that are controlled by the ESLint team and have no external dependencies can be safely loaded synchronously using [`require(esm)`](https://nodejs.org/api/modules.html#loading-ecmascript-modules-using-require) and therefore used in any contexts.

For external packages, we don't use `require(esm)` because a package could add a top-level `await` and thus break ESLint. We can use an external ESM-only package only in case it is needed only in asynchronous code, in which case it can be loaded using dynamic `import()`.

## License

MIT License

Copyright OpenJS Foundation and other contributors, <www.openjsf.org>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.

## Team

These folks keep the project moving and are resources for help.

<!-- NOTE: This section is autogenerated. Do not manually edit.-->

<!--teamstart-->

### Technical Steering Committee (TSC)

The people who manage releases, review feature requests, and meet regularly to ensure ESLint is properly maintained.

<table><tbody><tr><td align="center" valign="top" width="11%">
<a href="https://github.com/nzakas">
<img src="https://github.com/nzakas.png?s=75" width="75" height="75" alt="Nicholas C. Zakas's Avatar"><br />
Nicholas C. Zakas
</a>
</td><td align="center" valign="top" width="11%">
<a href="https://github.com/fasttime">
<img src="https://github.com/fasttime.png?s=75" width="75" height="75" alt="Francesco Trotta's Avatar"><br />
Francesco Trotta
</a>
</td><td align="center" valign="top" width="11%">
<a href="https://github.com/mdjermanovic">
<img src="https://github.com/mdjermanovic.png?s=75" width="75" height="75" alt="Milos Djermanovic's Avatar"><br />
Milos Djermanovic
</a>
</td></tr></tbody></table>

### Reviewers

The people who review and implement new features.

<table><tbody><tr><td align="center" valign="top" width="11%">
<a href="https://github.com/aladdin-add">
<img src="https://github.com/aladdin-add.png?s=75" width="75" height="75" alt="唯然's Avatar"><br />
唯然
</a>
</td><td align="center" valign="top" width="11%">
<a href="https://github.com/snitin315">
<img src="https://github.com/snitin315.png?s=75" width="75" height="75" alt="Nitin Kumar's Avatar"><br />
Nitin Kumar
</a>
</td></tr></tbody></table>

### Committers

The people who review and fix bugs and help triage issues.

<table><tbody><tr><td align="center" valign="top" width="11%">
<a href="https://github.com/DMartens">
<img src="https://github.com/DMartens.png?s=75" width="75" height="75" alt="fnx's Avatar"><br />
fnx
</a>
</td><td align="center" valign="top" width="11%">
<a href="https://github.com/JoshuaKGoldberg">
<img src="https://github.com/JoshuaKGoldberg.png?s=75" width="75" height="75" alt="Josh Goldberg ✨'s Avatar"><br />
Josh Goldberg ✨
</a>
</td><td align="center" valign="top" width="11%">
<a href="https://github.com/SwetaTanwar">
<img src="https://github.com/SwetaTanwar.png?s=75" width="75" height="75" alt="Sweta Tanwar's Avatar"><br />
Sweta Tanwar
</a>
</td><td align="center" valign="top" width="11%">
<a href="https://github.com/Tanujkanti4441">
<img src="https://github.com/Tanujkanti4441.png?s=75" width="75" height="75" alt="Tanuj Kanti's Avatar"><br />
Tanuj Kanti
</a>
</td><td align="center" valign="top" width="11%">
<a href="https://github.com/lumirlumir">
<img src="https://github.com/lumirlumir.png?s=75" width="75" height="75" alt="루밀LuMir's Avatar"><br />
루밀LuMir
</a>
</td><td align="center" valign="top" width="11%">
<a href="https://github.com/Pixel998">
<img src="https://github.com/Pixel998.png?s=75" width="75" height="75" alt="Pixel998's Avatar"><br />
Pixel998
</a>
</td></tr></tbody></table>

### Website Team

Team members who focus specifically on eslint.org

<table><tbody><tr><td align="center" valign="top" width="11%">
<a href="https://github.com/amareshsm">
<img src="https://github.com/amareshsm.png?s=75" width="75" height="75" alt="Amaresh  S M's Avatar"><br />
Amaresh  S M
</a>
</td><td align="center" valign="top" width="11%">
<a href="https://github.com/harish-sethuraman">
<img src="https://github.com/harish-sethuraman.png?s=75" width="75" height="75" alt="Harish's Avatar"><br />
Harish
</a>
</td><td align="center" valign="top" width="11%">
<a href="https://github.com/kecrily">
<img src="https://github.com/kecrily.png?s=75" width="75" height="75" alt="Percy Ma's Avatar"><br />
Percy Ma
</a>
</td></tr></tbody></table>

<!--teamend-->

<!-- NOTE: This section is autogenerated. Do not manually edit.-->
<!--sponsorsstart-->

## Sponsors

The following companies, organizations, and individuals support ESLint's ongoing maintenance and development. [Become a Sponsor](https://eslint.org/donate)
to get your logo on our READMEs and [website](https://eslint.org/sponsors).

<h3>Platinum Sponsors</h3>
<p><a href="https://automattic.com"><img src="https://images.opencollective.com/automattic/d0ef3e1/logo.png" alt="Automattic" height="128"></a></p><h3>Gold Sponsors</h3>
<p><a href="https://qlty.sh/"><img src="https://images.opencollective.com/qltysh/33d157d/logo.png" alt="Qlty Software" height="96"></a></p><h3>Silver Sponsors</h3>
<p><a href="https://vite.dev/"><img src="https://images.opencollective.com/vite/d472863/logo.png" alt="Vite" height="64"></a> <a href="https://liftoff.io/"><img src="https://images.opencollective.com/liftoff/2d6c3b6/logo.png" alt="Liftoff" height="64"></a> <a href="https://stackblitz.com"><img src="https://avatars.githubusercontent.com/u/28635252" alt="StackBlitz" height="64"></a></p><h3>Bronze Sponsors</h3>
<p><a href="https://syntax.fm"><img src="https://github.com/syntaxfm.png" alt="Syntax" height="32"></a> <a href="https://cybozu.co.jp/"><img src="https://images.opencollective.com/cybozu/933e46d/logo.png" alt="Cybozu" height="32"></a> <a href="https://opensource.sap.com"><img src="https://avatars.githubusercontent.com/u/2531208" alt="SAP" height="32"></a> <a href="https://www.crawljobs.com/"><img src="https://images.opencollective.com/crawljobs-poland/fa43a17/logo.png" alt="CrawlJobs" height="32"></a> <a href="https://depot.dev"><img src="https://images.opencollective.com/depot/39125a1/logo.png" alt="Depot" height="32"></a> <a href="https://www.n-ix.com/"><img src="https://images.opencollective.com/n-ix-ltd/575a7a5/logo.png" alt="N-iX Ltd" height="32"></a> <a href="https://icons8.com/"><img src="https://images.opencollective.com/icons8/7fa1641/logo.png" alt="Icons8" height="32"></a> <a href="https://discord.com"><img src="https://images.opencollective.com/discordapp/f9645d9/logo.png" alt="Discord" height="32"></a> <a href="https://www.gitbook.com"><img src="https://avatars.githubusercontent.com/u/7111340" alt="GitBook" height="32"></a> <a href="https://herocoders.com"><img src="https://avatars.githubusercontent.com/u/37549774" alt="HeroCoders" height="32"></a> <a href="https://www.lambdatest.com"><img src="https://avatars.githubusercontent.com/u/171592363" alt="TestMu AI Open Source Office (Formerly LambdaTest)" height="32"></a></p>
<h3>Technology Sponsors</h3>
Technology sponsors allow us to use their products and services for free as part of a contribution to the open source ecosystem and our work.
<p><a href="https://netlify.com"><img src="https://raw.githubusercontent.com/eslint/eslint.org/main/src/assets/images/techsponsors/netlify-icon.svg" alt="Netlify" height="32"></a> <a href="https://algolia.com"><img src="https://raw.githubusercontent.com/eslint/eslint.org/main/src/assets/images/techsponsors/algolia-icon.svg" alt="Algolia" height="32"></a> <a href="https://1password.com"><img src="https://raw.githubusercontent.com/eslint/eslint.org/main/src/assets/images/techsponsors/1password-icon.svg" alt="1Password" height="32"></a></p>

<!--sponsorsend-->

[tidelift]: https://tidelift.com/funding/github/npm/eslint
[herodevs]: https://www.herodevs.com/support/eslint-nes?utm_source=ESLintWebsite&utm_medium=ESLintWebsite&utm_campaign=ESLintNES&utm_id=ESLintNES
"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_framer-motion.md".into(),
                content: r####"# framer-motion

> Production-ready motion and physics gesture library for React
> **Version:** 12.40.0 · **Runtime:** bun · [npm](https://www.npmjs.com/package/framer-motion) · [Docs](https://framer.com/motion)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/framer-motion.md`
> To regenerate from original: `minicode skill reset framer-motion --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add framer-motion
```

---

## Usage

### Simple Transitions
Wrap any HTML element with `motion.` to enable animated properties:

```tsx
import { motion } from 'framer-motion';

export default function FadeIn() {
  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.6 }}
    >
      Hello Motion
    </motion.div>
  );
}
```

### Drag Gestures
Framer Motion handles touch and drag gestures with absolute ease:

```tsx
<motion.div
  drag
  dragConstraints={{ left: -100, right: 100, top: -50, bottom: 50 }}
  dragElastic={0.15}
  whileDrag={{ scale: 1.1, cursor: "grabbing" }}
  transition={{ type: "spring", stiffness: 300, damping: 20 }}
  className="w-16 h-16 bg-primary rounded-full cursor-grab"
/>
```

### Exit Animations (AnimatePresence)
```tsx
import { motion, AnimatePresence } from 'framer-motion';

export default function ToggleModal({ isOpen }: { isOpen: boolean }) {
  return (
    <AnimatePresence>
      {isOpen && (
        <motion.div
          initial={{ opacity: 0, scale: 0.9 }}
          animate={{ opacity: 1, scale: 1 }}
          exit={{ opacity: 0, scale: 0.9 }}
          className="modal"
        >
          Modal Content
        </motion.div>
      )}
    </AnimatePresence>
  );
}
```
"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_globals.md".into(),
                content: r###"# globals — minikit docs
> **Version**: 17.4.0 · **Runtime**: bun · **npm**: https://www.npmjs.com/package/globals

Global identifiers from different JavaScript environments
**Homepage**: https://github.com/sindresorhus/globals#readme

---

> ✏️ Edit this file freely — it lives in ~/.minikit/docs/bun/globals.md
> Every project you add globals to will get YOUR edited version.
> To regenerate from original: `minicode skill reset globals --runtime bun`

## My Notes

<!-- Add your own notes, snippets, team conventions here -->

---

# globals

> Global identifiers from different JavaScript environments

It's just a [JSON file](globals.json), so you can use it in any environment.

This package is used by ESLint 8 and earlier. For ESLint 9 and later, you should depend on this package directly in [your ESLint config](https://eslint.org/docs/latest/use/configure/language-options#predefined-global-variables).

## Install

```sh
npm install globals
```

## Usage

```js
import globals from 'globals';

console.log(globals.browser);
/*
{
	addEventListener: false,
	applicationCache: false,
	ArrayBuffer: false,
	atob: false,
	…
}
*/
```

Each global is given a value of `true` or `false`. A value of `true` indicates that the variable may be overwritten. A value of `false` indicates that the variable should be considered read-only. This information is used by static analysis tools to flag incorrect behavior. We assume all variables should be `false` unless we hear otherwise.

For Node.js this package provides two sets of globals:

- `globals.nodeBuiltin`: Globals available to all code running in Node.js.
	These will usually be available as properties on the `globalThis` object and include `process`, `Buffer`, but not CommonJS arguments like `require`.
	See: https://nodejs.org/api/globals.html
- `globals.node`: A combination of the globals from `nodeBuiltin` plus all CommonJS arguments ("CommonJS module scope").
	See: https://nodejs.org/api/modules.html#modules_the_module_scope

When analyzing code that is known to run outside of a CommonJS wrapper, for example, JavaScript modules, `nodeBuiltin` can find accidental CommonJS references.
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_gsap.md".into(),
                content: r####"# gsap

> Professional-grade animation and scroll-triggered timelines for modern web apps
> **Version:** 3.15.0 · **Runtime:** bun · [npm](https://www.npmjs.com/package/gsap) · [Docs](https://gsap.com/docs/v3/)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/gsap.md`
> To regenerate from original: `minicode skill reset gsap --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add gsap
```

---

## Usage

### Register Plugins
Always register `ScrollTrigger` or other plugins at your application root:

```typescript
import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

gsap.registerPlugin(ScrollTrigger);
```

### Simple Tween
```typescript
gsap.to(".box", { x: 200, duration: 1, ease: "power2.out" });
```

### Timelines
```typescript
const tl = gsap.timeline({ defaults: { ease: "power3.inOut" } });

tl.to(".title", { y: 0, opacity: 1, duration: 0.6 })
  .to(".card", { scale: 1, opacity: 1, stagger: 0.1, duration: 0.4 }, "-=0.3");
```

### ScrollTrigger
```typescript
gsap.to(".card-reveal", {
  opacity: 1,
  y: 0,
  stagger: 0.15,
  scrollTrigger: {
    trigger: ".grid-container",
    start: "top 80%", // plays when container top hits 80% of viewport height
    toggleActions: "play none none none",
  }
});
```
"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_lenis.md".into(),
                content: r####"# lenis

> High-performance smooth scroll library for modern browsers
> **Version:** 1.3.23 · **Runtime:** bun · [npm](https://www.npmjs.com/package/lenis) · [Docs](https://github.com/darkroomengineering/lenis)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/lenis.md`
> To regenerate from original: `minicode skill reset lenis --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add lenis
```

---

## Usage

### Simple React Setup
Initialize Lenis in a high-level component (like `RootLayout`):

```tsx
import { useEffect } from 'react';
import Lenis from 'lenis';

export function ScrollWrapper() {
  useEffect(() => {
    const lenis = new Lenis({
      autoRaf: true, // Auto runs requestAnimationFrame
      duration: 1.2,
      easing: (t) => Math.min(1, 1.001 - Math.pow(2, -10 * t)),
    });

    return () => {
      lenis.destroy();
    };
  }, []);

  return null;
}
```

### Syncing with GSAP ScrollTrigger
To ensure ScrollTrigger behaves correctly with smooth inertia scrolling:

```typescript
import { useEffect } from 'react';
import Lenis from 'lenis';
import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

gsap.registerPlugin(ScrollTrigger);

useEffect(() => {
  const lenis = new Lenis({ autoRaf: true });

  // Update ScrollTrigger coordinates on scroll
  lenis.on('scroll', ScrollTrigger.update);

  return () => {
    lenis.destroy();
  };
}, []);
```
"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_lordicons.md".into(),
                content: r####"# lordicons

> Highly-interactive, vector-based animated icons for user interfaces
> **Version:** 1.11.0 · **Runtime:** bun · [npm](https://www.npmjs.com/package/@lordicon/react) · [Docs](https://lordicon.com/docs/react)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/lordicons.md`
> To regenerate from original: `minicode skill reset lordicons --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add @lordicon/react lottie-web
```

---

## Usage

### Dynamic Lordicon Loader
Create a wrapper to dynamically fetch JSON data from URLs or CDN endpoints:

```tsx
import { useEffect, useRef, useState } from 'react';
import { Player } from '@lordicon/react';

export function LordIcon({ src, size = 32, trigger = 'hover' }) {
  const ref = useRef<Player>(null);
  const [data, setData] = useState(null);

  useEffect(() => {
    fetch(src).then(res => res.json()).then(setData);
  }, [src]);

  const onEnter = () => trigger === 'hover' && ref.current?.playFromBeginning();

  if (!data) return <div style={{ width: size, height: size }} className="animate-pulse bg-muted/20" />;

  return (
    <div onMouseEnter={onEnter} style={{ width: size, height: size }}>
      <Player ref={ref} icon={data} size={size} />
    </div>
  );
}
```

### Supported Trigger Constants
Triggers indicate when and how the vector animation plays:
*   `hover` — Plays from beginning once when hovered.
*   `click` — Plays once when clicked.
*   `loop` — Loops constantly.
*   `morph` — Transforms dynamically based on state transitions.
"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_lottie.md".into(),
                content: r####"# lottie

> Lightweight vector-based visual animations rendered in real-time
> **Version:** 2.4.1 · **Runtime:** bun · [npm](https://www.npmjs.com/package/lottie-react) · [Docs](https://github.com/LottieFiles/lottie-react)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/lottie.md`
> To regenerate from original: `minicode skill reset lottie --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add lottie-react
```

---

## Usage

### Simple Usage
```tsx
import Lottie from 'lottie-react';
import animationData from './assets/my-animation.json';

export function Illustration() {
  return (
    <div className="w-64 h-64">
      <Lottie animationData={animationData} loop={true} />
    </div>
  );
}
```

### Loading Lottie from a CDN Link
To prevent packaging heavy JSON files inside your bundle, fetch files dynamically:

```tsx
import { useEffect, useState } from 'react';
import Lottie from 'lottie-react';

export function LottieLoader({ src }: { src: string }) {
  const [data, setData] = useState(null);

  useEffect(() => {
    fetch(src).then(res => res.json()).then(setData);
  }, [src]);

  if (!data) return <div className="animate-pulse bg-muted/20 w-64 h-64" />;

  return <Lottie animationData={data} loop={true} />;
}
```

### Accessibility Compliance (Reduced Motion)
Respect preferences of users who request reduced motion:

```typescript
const prefersReduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
// Disable loops or animations if prefersReduced is true
```
"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_lucide-react.md".into(),
                content: r####"# lucide-react — minikit docs
> **Version**: 1.6.0 · **Runtime**: bun · **npm**: https://www.npmjs.com/package/lucide-react

A Lucide icon library package for React applications.
**Homepage**: https://lucide.dev

---

> ✏️ Edit this file freely — it lives in ~/.minikit/docs/bun/lucide-react.md
> Every project you add lucide-react to will get YOUR edited version.
> To regenerate from original: `minicode skill reset lucide-react --runtime bun`

## My Notes

<!-- Add your own notes, snippets, team conventions here -->

---

<p align="center">
  <a href="https://github.com/lucide-icons/lucide">
    <img src="https://lucide.dev/package-logos/lucide-react.svg" alt="Lucide icon library for React applications." width="540">
  </a>
</p>

<p align="center">
Lucide icon library for React applications.
</p>

<div align="center">

  [![npm](https://img.shields.io/npm/v/lucide-react?color=blue)](https://www.npmjs.com/package/lucide-react)
  ![NPM Downloads](https://img.shields.io/npm/dw/lucide-react)
  [![GitHub](https://img.shields.io/github/license/lucide-icons/lucide)](https://lucide.dev/license)
</div>

<p align="center">
  <a href="https://lucide.dev/guide/">About</a>
  ·
  <a href="https://lucide.dev/icons/">Icons</a>
  ·
  <a href="https://lucide.dev/guide/react">Documentation</a>
  ·
  <a href="https://lucide.dev/license">License</a>
</p>

# Lucide React

Implementation of the Lucide icon library for React applications.

## Installation

```sh
pnpm add lucide-react
```

```sh
npm install lucide-react
```

```sh
yarn add lucide-react
```

```sh
bun add lucide-react
```

## Documentation

For full documentation, visit [lucide.dev](https://lucide.dev/guide/packages/lucide-react)

## Community

Join the [Discord server](https://discord.gg/EH6nSts) to chat with the maintainers and other users.

## License

Lucide is licensed under the ISC license. See [LICENSE](https://lucide.dev/license).

## Sponsors

<a href="https://vercel.com?utm_source=lucide&utm_campaign=oss">
  <img src="https://lucide.dev/vercel.svg" alt="Powered by Vercel" width="200" />
</a>

<a href="https://www.digitalocean.com/?refcode=b0877a2caebd&utm_campaign=Referral_Invite&utm_medium=Referral_Program&utm_source=badge"><img src="https://lucide.dev/digitalocean.svg" width="200" alt="DigitalOcean Referral Badge" /></a>

[//]: <> (Open Collective backers)
### Awesome backers 🍺

<a href="https://github.com/pdfme/pdfme"><img src="https://lucide.dev/sponsors/pdfme.svg" width="180" alt="pdfme – Open-source PDF generation library built with TypeScript and React." /></a>
<a href="https://www.paxhistoria.co/"><img src="https://lucide.dev/sponsors/paxhistoria.svg?" width="180" alt="Pax Historia – An alternate history sandbox game" /></a>

### Backers ☕

<a href="https://www.fina.money/"><img src="https://lucide.dev/sponsors/fina-money.png" width="180" alt="Fina Money – Modular Finance Tracker" /></a>

### Other contributors 💸

You can find all our past and non-recurring financial contributors at [our Open Collective page](https://opencollective.com/lucide-icons).
"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_react-dom.md".into(),
                content: r###"# react-dom

> React renderer for the browser DOM
> **Version:** 19.2.4 · **Runtime:** bun · [npm](https://www.npmjs.com/package/react-dom) · [Docs](https://react.dev/reference/react-dom)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/react-dom.md`
> To regenerate from original: `minicode skill reset react-dom --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add react react-dom
bun add -d @types/react @types/react-dom
```

> Always install `react` and `react-dom` together — they must be the same version.

---

## Entry Point

```tsx
// main.tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>
)
```

---

## `createRoot` API

```tsx
import { createRoot } from 'react-dom/client'

const root = createRoot(document.getElementById('root')!)

// Render / update
root.render(<App />)

// Unmount and clean up
root.unmount()
```

---

## Portals — render outside the component tree

Useful for modals, tooltips, and overlays that need to escape CSS overflow or z-index constraints.

```tsx
import { createPortal } from 'react-dom'

function Modal({ children }: { children: React.ReactNode }) {
  return createPortal(
    <div className="modal-overlay">
      <div className="modal">{children}</div>
    </div>,
    document.body  // renders here, not in parent's DOM position
  )
}
```

---

## `flushSync` — force synchronous DOM update

Use sparingly — bypasses React's batching. Useful when you need the DOM updated before reading a layout measurement.

```tsx
import { flushSync } from 'react-dom'

flushSync(() => {
  setCount(count + 1)
})
// DOM is updated here before the next line
inputRef.current?.scrollIntoView()
```

---

## `useFormStatus` — form submission state (React 19)

Must be used inside a `<form>` with a Server Action.

```tsx
import { useFormStatus } from 'react-dom'

function SubmitButton() {
  const { pending } = useFormStatus()

  return (
    <button type="submit" disabled={pending}>
      {pending ? 'Saving...' : 'Save'}
    </button>
  )
}
```

---

## Hydration (SSR)

```tsx
import { hydrateRoot } from 'react-dom/client'

// Attach React to server-rendered HTML
hydrateRoot(
  document.getElementById('root')!,
  <App />
)
```

---

## Imports at a Glance

```tsx
// Client rendering
import { createRoot } from 'react-dom/client'
import { hydrateRoot } from 'react-dom/client'

// Utilities
import { createPortal } from 'react-dom'
import { flushSync } from 'react-dom'

// React 19 form hooks
import { useFormStatus } from 'react-dom'
import { useFormState } from 'react-dom'  // renamed to useActionState in React 19
```

---

## Resources

- [react-dom API Reference](https://react.dev/reference/react-dom)
- [createRoot](https://react.dev/reference/react-dom/client/createRoot)
- [Portals](https://react.dev/reference/react-dom/createPortal)"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_react-hook-form.md".into(),
                content: r###"# react-hook-form

> Performant, flexible form library for React using hooks
> **Version:** 7.72.0 · **Runtime:** bun · [npm](https://www.npmjs.com/package/react-hook-form) · [Docs](https://react-hook-form.com)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/react-hook-form.md`
> To regenerate from original: `minicode skill reset react-hook-form --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add react-hook-form
```

---

## Usage

```tsx
import { useForm } from 'react-hook-form'

type FormData = {
  email: string
  password: string
}

function LoginForm() {
  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm<FormData>()

  const onSubmit = async (data: FormData) => {
    await signIn(data)
  }

  return (
    <form onSubmit={handleSubmit(onSubmit)}>
      <input {...register('email', { required: 'Email is required' })} />
      {errors.email && <p>{errors.email.message}</p>}

      <input
        type="password"
        {...register('password', { minLength: { value: 8, message: 'Min 8 chars' } })}
      />
      {errors.password && <p>{errors.password.message}</p>}

      <button type="submit" disabled={isSubmitting}>
        {isSubmitting ? 'Logging in...' : 'Login'}
      </button>
    </form>
  )
}
```

---

## `register` — Validation Rules

```tsx
register('field', {
  required: 'This field is required',
  minLength: { value: 3, message: 'Min 3 characters' },
  maxLength: { value: 100, message: 'Max 100 characters' },
  min: { value: 0, message: 'Must be positive' },
  max: { value: 99, message: 'Max 99' },
  pattern: { value: /^\S+@\S+$/, message: 'Invalid email' },
  validate: (value) => value !== 'admin' || 'Username "admin" not allowed',
  validate: {
    notEmpty: (v) => v.trim().length > 0 || 'Cannot be blank',
    noSpaces: (v) => !v.includes(' ') || 'No spaces allowed',
  },
})
```

---

## `useForm` Options

```ts
const form = useForm<FormData>({
  defaultValues: {
    email: '',
    password: '',
  },
  mode: 'onSubmit',       // when to validate: 'onSubmit' | 'onBlur' | 'onChange' | 'all'
  reValidateMode: 'onChange',
  shouldFocusError: true,
  criteriaMode: 'firstError',  // 'all' to collect all errors per field
})
```

---

## `formState`

```ts
const { formState } = useForm()

formState.errors           // validation errors by field name
formState.isSubmitting     // true while handleSubmit is running
formState.isSubmitted      // true after first submit
formState.isDirty          // true if any field was changed
formState.isValid          // true if no errors
formState.touchedFields    // fields the user has interacted with
formState.dirtyFields      // fields that differ from defaultValues
```

---

## Utility Methods

```ts
const { reset, setValue, getValues, watch, setError, clearErrors, trigger } = useForm()

// Reset form to defaultValues (or new values)
reset()
reset({ email: 'new@example.com' })

// Set a field value
setValue('email', 'user@example.com')
setValue('email', 'user@example.com', { shouldValidate: true, shouldDirty: true })

// Get current values
const values = getValues()
const email = getValues('email')

// Watch field(s) for re-rendering
const email = watch('email')
const [email, password] = watch(['email', 'password'])

// Set a manual error
setError('email', { type: 'manual', message: 'Email already taken' })

// Trigger validation manually
await trigger()           // all fields
await trigger('email')    // specific field
```

---

## `Controller` — For UI Library Inputs

Use `Controller` when wrapping third-party components (Select, DatePicker, etc.):

```tsx
import { useForm, Controller } from 'react-hook-form'

<Controller
  name="country"
  control={control}
  rules={{ required: true }}
  render={({ field }) => (
    <Select {...field} options={countryOptions} />
  )}
/>
```

---

## Schema Validation (with Zod)

```bash
bun add @hookform/resolvers zod
```

```tsx
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'

const schema = z.object({
  email: z.string().email(),
  password: z.string().min(8),
})

type FormData = z.infer<typeof schema>

const { register, handleSubmit, formState: { errors } } = useForm<FormData>({
  resolver: zodResolver(schema),
})
```

---

## Resources

- [Official Docs](https://react-hook-form.com)
- [API Reference](https://react-hook-form.com/docs)
- [Resolvers (Zod, Yup, Joi...)](https://github.com/react-hook-form/resolvers)"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_react-router-dom.md".into(),
                content: r###"# react-router-dom — minikit docs
> **Version**: 7.13.2 · **Runtime**: bun · **npm**: https://www.npmjs.com/package/react-router-dom

Declarative routing for React web applications
**Homepage**: https://github.com/remix-run/react-router#readme

---

> ✏️ Edit this file freely — it lives in ~/.minikit/docs/bun/react-router-dom.md
> Every project you add react-router-dom to will get YOUR edited version.
> To regenerate from original: `minicode skill reset react-router-dom --runtime bun`

## My Notes

<!-- Add your own notes, snippets, team conventions here -->

---

## Installation

```json
{ "dependencies": { "react-router-dom": "^7.13.2" } }
```

## Import

```typescript
import ... from 'react-router-dom';
```

## Quick Start

```typescript
// Add your usage example here
```

## Links

- npm: https://www.npmjs.com/package/react-router-dom
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_react.md".into(),
                content: r####"# react

> JavaScript library for building user interfaces
> **Version:** 19.2.4 · **Runtime:** bun · [npm](https://www.npmjs.com/package/react) · [Docs](https://react.dev)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/react.md`
> To regenerate from original: `minicode skill reset react --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add react react-dom
bun add -d @types/react @types/react-dom
```

## Import

```tsx
import React, { useState, useEffect, useRef, useMemo, useCallback } from 'react'
import ReactDOM from 'react-dom/client'
```

---

## Component

```tsx
// Function component (standard)
function Greeting({ name }: { name: string }) {
  return <h1>Hello, {name}!</h1>
}

// Arrow function component
const Button = ({ onClick, children }: { onClick: () => void; children: React.ReactNode }) => (
  <button onClick={onClick}>{children}</button>
)
```

---

## Hooks

### `useState`

```tsx
const [count, setCount] = useState(0)
const [user, setUser] = useState<User | null>(null)

setCount(count + 1)
setCount(prev => prev + 1)          // functional update
setUser(prev => ({ ...prev, name: 'Jane' }))
```

### `useEffect`

```tsx
// On mount
useEffect(() => {
  fetchData()
}, [])

// On dependency change
useEffect(() => {
  document.title = `${count} items`
}, [count])

// Cleanup on unmount
useEffect(() => {
  const sub = subscribe()
  return () => sub.unsubscribe()
}, [])
```

### `useRef`

```tsx
const inputRef = useRef<HTMLInputElement>(null)
inputRef.current?.focus()

// Mutable value — does NOT trigger re-render
const timerRef = useRef<number>(0)
timerRef.current = setTimeout(fn, 1000)
```

### `useMemo` — expensive computation cache

```tsx
const sorted = useMemo(
  () => items.slice().sort((a, b) => a.name.localeCompare(b.name)),
  [items]
)
```

### `useCallback` — stable function reference

```tsx
const handleClick = useCallback(() => {
  setCount(c => c + 1)
}, []) // only recreated if deps change
```

### `useContext`

```tsx
const ThemeContext = React.createContext<'light' | 'dark'>('light')

// Provide
<ThemeContext.Provider value="dark">
  <App />
</ThemeContext.Provider>

// Consume
const theme = useContext(ThemeContext)
```

### `useReducer`

```tsx
type Action = { type: 'increment' } | { type: 'reset' }

function reducer(state: number, action: Action): number {
  switch (action.type) {
    case 'increment': return state + 1
    case 'reset': return 0
  }
}

const [count, dispatch] = useReducer(reducer, 0)
dispatch({ type: 'increment' })
```

### `useId` — unique IDs for accessibility

```tsx
const id = useId()
<label htmlFor={id}>Name</label>
<input id={id} />
```

### `useTransition` — mark state update as non-urgent

```tsx
const [isPending, startTransition] = useTransition()

startTransition(() => {
  setFilter(input)  // non-blocking update
})
```

---

## React 19 — New Features

### `use()` hook — read resources in render

```tsx
import { use } from 'react'

function UserCard({ userPromise }: { userPromise: Promise<User> }) {
  const user = use(userPromise)  // suspends until resolved
  return <p>{user.name}</p>
}
```

### `useOptimistic` — optimistic UI updates

```tsx
const [optimisticLikes, addOptimisticLike] = useOptimistic(
  likes,
  (state, newLike) => [...state, newLike]
)

async function handleLike() {
  addOptimisticLike(tempLike)   // shows immediately
  await saveLike(tempLike)      // actual API call
}
```

### Server Actions (React 19 + frameworks)

```tsx
async function createUser(formData: FormData) {
  'use server'
  const name = formData.get('name')
  await db.users.create({ name })
}

<form action={createUser}>
  <input name="name" />
  <button type="submit">Create</button>
</form>
```

---

## JSX Patterns

```tsx
// Conditional rendering
{isLoggedIn && <Dashboard />}
{isLoggedIn ? <Dashboard /> : <Login />}

// Lists
{items.map(item => (
  <li key={item.id}>{item.name}</li>
))}

// Fragment (no extra DOM node)
<>
  <h1>Title</h1>
  <p>Body</p>
</>

// Spread props
<Button {...buttonProps} />

// Children
function Card({ children }: { children: React.ReactNode }) {
  return <div className="card">{children}</div>
}
```

---

## TypeScript Patterns

```tsx
// Component props
interface ButtonProps {
  label: string
  onClick: () => void
  variant?: 'primary' | 'secondary'
  disabled?: boolean
  children?: React.ReactNode
}

// Event handlers
const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {}
const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {}
const handleClick = (e: React.MouseEvent<HTMLButtonElement>) => {}
const handleKeyDown = (e: React.KeyboardEvent) => {}

// Ref types
const divRef = useRef<HTMLDivElement>(null)
const inputRef = useRef<HTMLInputElement>(null)

// Generic component
function List<T extends { id: string }>({ items, renderItem }: {
  items: T[]
  renderItem: (item: T) => React.ReactNode
}) {
  return <ul>{items.map(item => <li key={item.id}>{renderItem(item)}</li>)}</ul>
}
```

---

## Entry Point (`react-dom`)

```tsx
// main.tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>
)
```

---

## Custom Hooks

```tsx
function useLocalStorage<T>(key: string, initial: T) {
  const [value, setValue] = useState<T>(() => {
    const stored = localStorage.getItem(key)
    return stored ? JSON.parse(stored) : initial
  })

  const set = useCallback((val: T) => {
    setValue(val)
    localStorage.setItem(key, JSON.stringify(val))
  }, [key])

  return [value, set] as const
}
```

---

## Resources

- [react.dev](https://react.dev)
- [Hooks Reference](https://react.dev/reference/react)
- [React 19 Changelog](https://react.dev/blog/2024/12/05/react-19)"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_shadcn.md".into(),
                content: r####"# shadcn

> Beautiful, accessible, fully-customizable UI components built on Radix Primitives
> **Version:** 4.10.0 · **Runtime:** bun · [npm](https://www.npmjs.com/package/shadcn) · [Docs](https://ui.shadcn.com)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/shadcn.md`
> To regenerate from original: `minicode skill reset shadcn --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bunx shadcn@latest init
bunx shadcn@latest add accordion tabs dialog
```

---

## Usage

### The Class Merger (`cn`)
All shadcn components utilize a custom Tailwind utility to safely merge classes without duplicates:

```typescript
// src/lib/utils.ts
import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}
```

### Rendering a Component (e.g. Accordion)
```tsx
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion"

export function AccordionDemo() {
  return (
    <Accordion type="single" collapsible>
      <AccordionItem value="item-1">
        <AccordionTrigger>Is it styled?</AccordionTrigger>
        <AccordionContent>
          Yes. It aligns with your Tailwind CSS variables theme out of the box.
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  )
}
```
"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_tailwindcss.md".into(),
                content: r####"# tailwindcss

> Utility-first CSS framework
> **Version:** 4.2.2 · **Runtime:** bun · [Docs](https://tailwindcss.com) · [Cheat sheet](https://nerdcave.com/tailwind-cheat-sheet)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/tailwindcss.md`
> To regenerate from original: `minicode skill reset tailwindcss --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install & Setup (v4)

```bash
bun add -d tailwindcss @tailwindcss/vite   # with Vite
# or
bun add -d tailwindcss @tailwindcss/postcss postcss
```

```ts
// vite.config.ts
import tailwindcss from '@tailwindcss/vite'
plugins: [tailwindcss()]
```

```css
/* globals.css — v4: one import replaces all three directives */
@import "tailwindcss";

/* Custom theme tokens — generates bg-primary, text-primary, etc. */
@theme {
  --color-primary: #3b82f6;
  --color-secondary: #8b5cf6;
  --font-sans: 'Inter', sans-serif;
  --spacing-18: 4.5rem;
}
```

> **v4 key change:** No `tailwind.config.js` needed. All config lives in your CSS file via `@theme {}`.

---

## SPACING SCALE

Default scale: `0 0.5 1 1.5 2 2.5 3 3.5 4 5 6 7 8 9 10 11 12 14 16 20 24 28 32 36 40 44 48 52 56 60 64 72 80 96`
Each unit = `0.25rem` (4px). So `p-4` = `1rem` = `16px`, `p-8` = `2rem` = `32px`.

**Quick mental model:**
```
p-1  =  4px      p-2  =  8px      p-4  =  16px
p-6  =  24px     p-8  =  32px     p-12 =  48px
p-16 =  64px     p-24 =  96px
```

---

## LAYOUT

### Display
```
block inline inline-block flex inline-flex grid inline-grid
table hidden contents
```

**Examples:**
```html
<!-- Card layout -->
<div class="flex gap-4">
  <div class="hidden md:block">Sidebar</div>  <!-- hide on mobile -->
  <main class="flex-1">Content</main>
</div>

<!-- Inline badge -->
<span class="inline-block px-2 py-0.5 text-xs bg-blue-100 text-blue-700 rounded-full">
  New
</span>
```

### Position
```
static relative absolute fixed sticky
inset-0  inset-x-0  inset-y-0
top-0  right-0  bottom-0  left-0
top-4  -top-2  top-1/2
z-0  z-10  z-20  z-30  z-40  z-50  z-auto  -z-10
```

**Examples:**
```html
<!-- Overlay modal -->
<div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center">
  <div class="relative bg-white rounded-xl p-6 w-full max-w-md">
    <button class="absolute top-4 right-4">✕</button>
    Modal content
  </div>
</div>

<!-- Sticky header -->
<header class="sticky top-0 z-40 bg-white border-b shadow-sm">...</header>

<!-- Badge on avatar -->
<div class="relative inline-block">
  <img class="w-10 h-10 rounded-full" src="..." />
  <span class="absolute -top-1 -right-1 w-3 h-3 bg-green-500 rounded-full border-2 border-white"></span>
</div>
```

### Overflow
```
overflow-hidden  overflow-auto  overflow-scroll  overflow-visible
overflow-x-hidden  overflow-y-auto
```

**Examples:**
```html
<!-- Horizontal scrollable tabs -->
<div class="flex overflow-x-auto gap-2 pb-2 scrollbar-hide">
  <button class="shrink-0 px-4 py-2">Tab 1</button>
  <button class="shrink-0 px-4 py-2">Tab 2</button>
</div>

<!-- Clip image in card -->
<div class="rounded-xl overflow-hidden">
  <img class="w-full h-48 object-cover" src="..." />
</div>
```

---

## FLEXBOX

```
flex          flex-row         flex-col
flex-wrap     flex-nowrap      flex-wrap-reverse
flex-1        flex-auto        flex-none        flex-initial

justify-start   justify-center   justify-end
justify-between  justify-around  justify-evenly  justify-stretch

items-start   items-center   items-end   items-baseline  items-stretch

self-start    self-center    self-end    self-auto

grow          grow-0
shrink        shrink-0

gap-4         gap-x-4         gap-y-4
```

**Examples:**
```html
<!-- Navbar -->
<nav class="flex items-center justify-between px-6 py-4">
  <span class="font-bold text-lg">Logo</span>
  <div class="flex items-center gap-4">
    <a href="#">About</a>
    <button class="bg-blue-600 text-white px-4 py-2 rounded-lg">Sign in</button>
  </div>
</nav>

<!-- Card with footer pinned to bottom -->
<div class="flex flex-col h-full">
  <div class="flex-1 p-4">Content grows here</div>
  <div class="p-4 border-t">Footer stays at bottom</div>
</div>

<!-- Centered hero -->
<section class="flex flex-col items-center justify-center min-h-screen text-center">
  <h1 class="text-5xl font-bold">Hello</h1>
  <p class="mt-4 text-gray-500 max-w-md">Subtitle goes here</p>
</section>

<!-- Tags that wrap -->
<div class="flex flex-wrap gap-2">
  <span class="px-3 py-1 bg-gray-100 rounded-full text-sm">React</span>
  <span class="px-3 py-1 bg-gray-100 rounded-full text-sm">TypeScript</span>
</div>
```

---

## GRID

```
grid
grid-cols-1  grid-cols-2  grid-cols-3 ... grid-cols-12  grid-cols-none
grid-rows-1  grid-rows-2 ...

col-span-1   col-span-2   col-span-full
col-start-1  col-end-3
row-span-2   row-start-1

gap-4    gap-x-4    gap-y-4

auto-cols-auto   auto-cols-fr   auto-cols-min   auto-cols-max
auto-rows-auto   auto-rows-fr
```

**Examples:**
```html
<!-- Responsive card grid -->
<div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
  <div class="bg-white rounded-xl shadow p-4">Card 1</div>
  <div class="bg-white rounded-xl shadow p-4">Card 2</div>
  <div class="bg-white rounded-xl shadow p-4">Card 3</div>
</div>

<!-- Dashboard layout with sidebar -->
<div class="grid grid-cols-[240px_1fr] min-h-screen">
  <aside class="bg-gray-900 text-white p-4">Sidebar</aside>
  <main class="p-6">Main content</main>
</div>

<!-- Feature grid with featured item spanning 2 cols -->
<div class="grid grid-cols-3 gap-4">
  <div class="col-span-2 bg-blue-50 rounded-xl p-6">Featured</div>
  <div class="bg-gray-50 rounded-xl p-6">Side</div>
  <div class="bg-gray-50 rounded-xl p-6">Item</div>
  <div class="bg-gray-50 rounded-xl p-6">Item</div>
  <div class="bg-gray-50 rounded-xl p-6">Item</div>
</div>
```

---

## SIZING

### Width
```
w-0  w-px  w-0.5  w-1 ... w-96
w-auto  w-full  w-screen  w-svw  w-dvw
w-1/2  w-1/3  w-2/3  w-1/4  w-3/4
w-fit  w-min  w-max
w-[350px]  w-[50%]  w-[calc(100%-2rem)]
```

### Height
```
h-0 h-px ... h-96
h-auto  h-full  h-screen  h-svh  h-dvh
h-1/2  h-1/3  h-fit  h-min  h-max
h-[500px]
```

> **Tip:** Use `h-svh` / `w-svw` instead of `h-screen` on mobile — avoids the browser toolbar overlap issue.

### Min / Max
```
min-w-0   min-w-full   min-w-min   min-w-max   min-w-fit
max-w-xs  max-w-sm  max-w-md  max-w-lg  max-w-xl
max-w-2xl  max-w-3xl  max-w-4xl  max-w-5xl  max-w-6xl  max-w-7xl
max-w-full  max-w-screen-xl  max-w-none  max-w-prose
min-h-0   min-h-full   min-h-screen
max-h-full  max-h-screen  max-h-96
```

**Examples:**
```html
<!-- Centered page container -->
<div class="max-w-5xl mx-auto px-4">Page content</div>

<!-- Readable prose width -->
<article class="max-w-prose mx-auto">Long form text...</article>

<!-- Avatar sizes -->
<img class="w-8 h-8 rounded-full" />   <!-- small: 32px -->
<img class="w-10 h-10 rounded-full" /> <!-- medium: 40px -->
<img class="w-12 h-12 rounded-full" /> <!-- large: 48px -->

<!-- Full-height sidebar that scrolls -->
<aside class="w-64 min-h-screen overflow-y-auto">...</aside>
```

---

## SPACING

### Padding
```
p-4           px-4 py-4
pt-4 pr-4 pb-4 pl-4
ps-4 pe-4     (logical: start/end)
p-[1.25rem]
```

### Margin
```
m-4  mx-4  my-4  mx-auto
mt-4  mr-4  mb-4  ml-4
-mt-2  -mx-4
m-[20px]
```

### Space Between (flex/grid children)
```
space-x-4   space-y-4
space-x-reverse   space-y-reverse
```

> **`space-x` vs `gap`:** Prefer `gap` on flex/grid containers. `space-x` adds margin to children and can break with wrapping. `gap` is cleaner and works correctly with `flex-wrap`.

**Examples:**
```html
<!-- Section spacing -->
<section class="py-16 px-4">
  <h2 class="text-3xl font-bold mb-2">Title</h2>
  <p class="text-gray-500 mb-8">Subtitle</p>
  <div class="grid grid-cols-3 gap-6">...</div>
</section>

<!-- Button with icon -->
<button class="flex items-center gap-2 px-4 py-2 bg-blue-600 text-white rounded-lg">
  <svg class="w-4 h-4">...</svg>
  <span>Download</span>
</button>

<!-- Negative margin pull trick -->
<div class="mt-8 -mx-4 px-4 bg-gray-50 py-6">
  Full-bleed section in a constrained container
</div>
```

---

## TYPOGRAPHY

### Font Size
```
text-xs    (0.75rem / 12px)
text-sm    (0.875rem / 14px)
text-base  (1rem / 16px)
text-lg    (1.125rem / 18px)
text-xl    (1.25rem / 20px)
text-2xl   (1.5rem / 24px)
text-3xl   (1.875rem / 30px)
text-4xl   (2.25rem / 36px)
text-5xl   (3rem / 48px)
text-6xl   (3.75rem / 60px)
text-7xl   (4.5rem / 72px)
text-8xl   (6rem / 96px)
text-9xl   (8rem / 128px)
text-[1.375rem]
```

### Font Weight
```
font-thin (100)    font-extralight (200)  font-light (300)
font-normal (400)  font-medium (500)      font-semibold (600)
font-bold (700)    font-extrabold (800)   font-black (900)
```

### Font Family
```
font-sans   font-serif   font-mono
```

### Text Alignment
```
text-left   text-center   text-right   text-justify   text-start   text-end
```

### Text Transform
```
uppercase   lowercase   capitalize   normal-case
```

### Text Decoration
```
underline   overline   line-through   no-underline
decoration-solid   decoration-dashed   decoration-dotted   decoration-double
decoration-blue-500   decoration-2
```

### Line Height
```
leading-none (1)    leading-tight (1.25)   leading-snug (1.375)
leading-normal (1.5) leading-relaxed (1.625) leading-loose (2)
leading-[1.8]
```

### Letter Spacing
```
tracking-tighter  tracking-tight  tracking-normal
tracking-wide     tracking-wider  tracking-widest
tracking-[0.05em]
```

### Truncate / Clamp
```
truncate         (single line ellipsis)
line-clamp-2     (clamp to 2 lines)
line-clamp-3
overflow-hidden
```

### Whitespace
```
whitespace-normal   whitespace-nowrap   whitespace-pre
whitespace-pre-wrap  whitespace-pre-line  whitespace-break-spaces
```

**Examples:**
```html
<!-- Page heading -->
<h1 class="text-4xl md:text-6xl font-black tracking-tight leading-tight">
  Build faster.
</h1>

<!-- Subheading with muted color -->
<p class="text-lg text-gray-500 leading-relaxed max-w-xl">
  A utility-first CSS framework packed with classes.
</p>

<!-- Label / caption -->
<span class="text-xs font-medium uppercase tracking-widest text-gray-400">
  Category
</span>

<!-- Card title that truncates -->
<h3 class="text-base font-semibold truncate">
  This is a very long title that will be cut off
</h3>

<!-- Multi-line clamp for card descriptions -->
<p class="text-sm text-gray-600 line-clamp-3">
  Long description text that will be clamped after 3 lines...
</p>

<!-- Monospace code block -->
<code class="font-mono text-sm bg-gray-100 px-2 py-0.5 rounded">
  bun run dev
</code>

<!-- Price display -->
<div class="flex items-baseline gap-1">
  <span class="text-3xl font-bold">$29</span>
  <span class="text-gray-400 line-through text-sm">$49</span>
</div>
```

---

## COLORS

### Text
```
text-black   text-white   text-transparent   text-current
text-slate-500   text-gray-700   text-zinc-900
text-red-500     text-orange-500  text-amber-500  text-yellow-500
text-lime-500    text-green-500   text-emerald-500 text-teal-500
text-cyan-500    text-sky-500     text-blue-500   text-indigo-500
text-violet-500  text-purple-500  text-fuchsia-500 text-pink-500 text-rose-500
text-blue-500/75  (with 75% opacity)
```

Shades: `50 100 200 300 400 500 600 700 800 900 950`

### Background
```
bg-white   bg-black   bg-transparent
bg-blue-500   bg-blue-500/50    (50% opacity)
bg-gradient-to-r  from-blue-500  via-purple-500  to-pink-500
bg-gradient-to-br  from-slate-900  to-slate-700
```

### Background Size / Position
```
bg-auto   bg-cover   bg-contain
bg-center bg-top bg-bottom bg-left bg-right
bg-no-repeat  bg-repeat  bg-repeat-x  bg-repeat-y
```

### Border Color
```
border-gray-200   border-blue-500   border-transparent
```

**Examples:**
```html
<!-- Status badges -->
<span class="bg-green-100 text-green-700 px-2 py-0.5 rounded-full text-xs font-medium">Active</span>
<span class="bg-yellow-100 text-yellow-700 px-2 py-0.5 rounded-full text-xs font-medium">Pending</span>
<span class="bg-red-100 text-red-700 px-2 py-0.5 rounded-full text-xs font-medium">Error</span>
<span class="bg-gray-100 text-gray-600 px-2 py-0.5 rounded-full text-xs font-medium">Draft</span>

<!-- Gradient hero background -->
<section class="bg-gradient-to-br from-slate-900 via-purple-900 to-slate-900 text-white py-24">
  <h1 class="text-5xl font-bold">Hello</h1>
</section>

<!-- Gradient text -->
<h1 class="bg-gradient-to-r from-blue-600 to-violet-600 bg-clip-text text-transparent text-5xl font-black">
  Gradient Text
</h1>

<!-- Semi-transparent overlay -->
<div class="absolute inset-0 bg-black/60 rounded-xl"></div>

<!-- Glass card -->
<div class="bg-white/10 backdrop-blur-md border border-white/20 rounded-xl p-6">
  Glass effect
</div>

<!-- Hero image with overlay text -->
<div class="relative h-64 rounded-xl overflow-hidden">
  <img class="w-full h-full object-cover" src="..." />
  <div class="absolute inset-0 bg-gradient-to-t from-black/70 to-transparent"></div>
  <p class="absolute bottom-4 left-4 text-white font-semibold">Caption text</p>
</div>
```

---

## BORDERS

```
border        (1px solid)
border-2      border-4      border-8
border-0      border-t-2    border-b    border-l-4    border-r

border-solid  border-dashed  border-dotted  border-double  border-none

border-gray-300   border-red-500

rounded-none   rounded-sm   rounded   rounded-md   rounded-lg
rounded-xl     rounded-2xl  rounded-3xl   rounded-full

rounded-t-lg   rounded-b-xl   rounded-l-md   rounded-r-full
rounded-tl-lg  rounded-tr-xl  rounded-bl-md  rounded-br-full
```

**Examples:**
```html
<!-- Card with subtle border -->
<div class="border border-gray-200 rounded-xl p-5 bg-white shadow-sm">Card</div>

<!-- Input field -->
<input class="w-full border border-gray-300 rounded-lg px-3 py-2
              focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent" />

<!-- Input with error state -->
<input class="border-2 border-red-400 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-red-400" />

<!-- Divider line -->
<hr class="border-t border-gray-100 my-6" />

<!-- Left accent border -->
<div class="border-l-4 border-blue-500 pl-4 text-gray-700">
  Info message with accent
</div>

<!-- Avatar / circle -->
<img class="w-12 h-12 rounded-full border-2 border-white ring-2 ring-blue-500" />

<!-- Pill / tag -->
<span class="border border-gray-300 rounded-full px-3 py-1 text-sm">Tag</span>

<!-- Dashed upload zone -->
<div class="border-2 border-dashed border-gray-300 rounded-xl p-8 text-center
            hover:border-blue-400 transition-colors cursor-pointer">
  Drop files here
</div>
```

---

## SHADOWS

```
shadow-xs    shadow-sm    shadow    shadow-md
shadow-lg    shadow-xl    shadow-2xl   shadow-inner   shadow-none
shadow-blue-500/50    (colored shadow)
```

### Ring (outline ring)
```
ring         (3px)
ring-1  ring-2  ring-4  ring-8
ring-blue-500    ring-offset-2   ring-offset-white
ring-inset
```

**Examples:**
```html
<!-- Elevated card -->
<div class="bg-white rounded-xl shadow-md hover:shadow-xl transition-shadow p-5">
  Lifts on hover
</div>

<!-- Floating action button -->
<button class="bg-blue-600 text-white rounded-full p-4 shadow-lg shadow-blue-500/40
               hover:shadow-xl hover:shadow-blue-500/50 transition-all">
  <svg class="w-6 h-6">...</svg>
</button>

<!-- Focused input ring -->
<input class="border border-gray-300 rounded-lg px-3 py-2
              focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2" />

<!-- Inset shadow (like pressed/sunken) -->
<div class="bg-gray-100 rounded-lg shadow-inner p-4">Inset input</div>
```

---

## EFFECTS

### Opacity
```
opacity-0   opacity-5   opacity-10   opacity-20   opacity-25
opacity-50  opacity-75  opacity-90   opacity-95   opacity-100
opacity-[0.35]
```

### Background Opacity (v4: use / modifier)
```
bg-black/50     text-white/80     border-gray-500/30
```

### Filter
```
blur-none  blur-sm  blur  blur-md  blur-lg  blur-xl  blur-2xl  blur-3xl
blur-[4px]
brightness-0  brightness-50  brightness-75  brightness-100  brightness-125  brightness-150
contrast-0   contrast-50   contrast-100   contrast-150   contrast-200
grayscale   grayscale-0
invert   invert-0
sepia    sepia-0
drop-shadow-md   drop-shadow-lg
backdrop-blur-sm  backdrop-blur-md  backdrop-blur-xl
backdrop-brightness-50  backdrop-contrast-150
```

**Examples:**
```html
<!-- Disabled state -->
<button class="opacity-50 cursor-not-allowed" disabled>Disabled</button>

<!-- Image hover: reveal on hover -->
<div class="group relative overflow-hidden rounded-xl">
  <img class="w-full h-64 object-cover transition-transform duration-300 group-hover:scale-105" />
  <div class="absolute inset-0 bg-black/0 group-hover:bg-black/40 transition-colors duration-300 flex items-center justify-center">
    <span class="text-white font-semibold opacity-0 group-hover:opacity-100 transition-opacity">View</span>
  </div>
</div>

<!-- Skeleton loader -->
<div class="animate-pulse">
  <div class="h-4 bg-gray-200 rounded w-3/4 mb-2"></div>
  <div class="h-4 bg-gray-200 rounded w-1/2"></div>
</div>

<!-- Glassmorphism card -->
<div class="backdrop-blur-xl bg-white/20 border border-white/30 rounded-2xl p-6">
  Glass card
</div>

<!-- Blurred background behind modal -->
<div class="fixed inset-0 backdrop-blur-sm bg-black/30 z-40"></div>

<!-- Grayscale image that colors on hover -->
<img class="grayscale hover:grayscale-0 transition-all duration-300" />
```

---

## TRANSFORMS

```
scale-0   scale-50   scale-75   scale-90   scale-95   scale-100   scale-105   scale-110   scale-125   scale-150
scale-x-0  scale-y-150
-scale-x-100   (flip horizontal)

rotate-0   rotate-1   rotate-2   rotate-3   rotate-6
rotate-12  rotate-45  rotate-90  rotate-180
-rotate-6  rotate-[17deg]

translate-x-4   translate-y-4  -translate-x-1/2   translate-y-1/2
translate-x-full  -translate-y-full

skew-x-2   skew-y-3

origin-center  origin-top  origin-top-right  origin-right
origin-bottom  origin-bottom-left  origin-left  origin-top-left
```

**Examples:**
```html
<!-- Button press effect -->
<button class="active:scale-95 transition-transform">Click me</button>

<!-- Hover lift -->
<div class="hover:-translate-y-1 transition-transform">Card</div>

<!-- Centered absolutely with translate trick -->
<div class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2">
  Perfectly centered
</div>

<!-- Rotate icon on state change -->
<svg class="transition-transform duration-200 rotate-0 group-open:rotate-180">
  <!-- chevron icon -->
</svg>

<!-- Flip horizontally (mirror) -->
<svg class="-scale-x-100">...</svg>

<!-- Card tilt on hover (with group) -->
<div class="group cursor-pointer">
  <div class="transition-transform duration-300 group-hover:rotate-1 group-hover:scale-105">
    Card content
  </div>
</div>
```

---

## TRANSITIONS & ANIMATION

```
transition-none   transition-all   transition   transition-colors
transition-opacity  transition-shadow  transition-transform

duration-75   duration-100   duration-150   duration-200
duration-300  duration-500   duration-700   duration-1000

ease-linear   ease-in   ease-out   ease-in-out

delay-0   delay-75   delay-100   delay-150   delay-300

animate-none
animate-spin      (rotate 360° loop)
animate-ping      (scale + fade loop — notifications)
animate-pulse     (opacity pulse — skeleton)
animate-bounce    (bounce loop)
```

> **Rule of thumb:** Use `duration-150` to `duration-200` for interactions (hover, click). Use `duration-300` to `duration-500` for entrance/exit animations. Faster feels snappier, slower feels heavier.

**Examples:**
```html
<!-- Smooth color + shadow on hover -->
<button class="bg-blue-600 text-white px-4 py-2 rounded-lg
               transition-all duration-200
               hover:bg-blue-700 hover:shadow-lg hover:shadow-blue-500/30
               active:scale-95">
  Button
</button>

<!-- Notification dot (ping) -->
<span class="relative flex h-3 w-3">
  <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-red-400 opacity-75"></span>
  <span class="relative inline-flex rounded-full h-3 w-3 bg-red-500"></span>
</span>

<!-- Loading spinner -->
<svg class="animate-spin h-5 w-5 text-blue-600">
  <!-- spinner SVG path -->
</svg>

<!-- Skeleton loading card -->
<div class="animate-pulse rounded-xl bg-gray-200 h-40 w-full"></div>

<!-- Staggered fade-in (use with JS class toggle) -->
<div class="opacity-0 translate-y-4 transition-all duration-500 delay-100 [&.visible]:opacity-100 [&.visible]:translate-y-0">
  Item 1
</div>

<!-- Transition only specific properties -->
<div class="transition-colors duration-200 bg-gray-100 hover:bg-blue-50">
  Only color transitions — no layout jitter
</div>
```

---

## INTERACTIVITY

```
cursor-auto   cursor-default   cursor-pointer   cursor-wait
cursor-text   cursor-move      cursor-not-allowed   cursor-grab  cursor-grabbing

select-none   select-text   select-all   select-auto

pointer-events-none   pointer-events-auto

resize   resize-none   resize-x   resize-y

scroll-auto   scroll-smooth
scroll-m-4   scroll-p-4    (scroll margin/padding)
snap-start   snap-center   snap-end
snap-x   snap-y   snap-mandatory   snap-proximity
```

**Examples:**
```html
<!-- Drag handle -->
<div class="cursor-grab active:cursor-grabbing p-2 text-gray-400">⋮⋮</div>

<!-- Non-interactive overlay (clicks pass through) -->
<div class="pointer-events-none absolute inset-0 border-2 border-blue-500 rounded-xl"></div>

<!-- Prevent text selection on double-click (e.g. buttons) -->
<button class="select-none px-4 py-2">Click me</button>

<!-- Smooth scroll to section -->
<html class="scroll-smooth">
<!-- then: -->
<a href="#section">Go to section</a>
<section id="section" class="scroll-mt-20">  <!-- offset for sticky header -->

<!-- Horizontal snap carousel -->
<div class="flex overflow-x-auto snap-x snap-mandatory gap-4 pb-4">
  <div class="snap-center shrink-0 w-80 bg-white rounded-xl p-4 shadow">Slide 1</div>
  <div class="snap-center shrink-0 w-80 bg-white rounded-xl p-4 shadow">Slide 2</div>
  <div class="snap-center shrink-0 w-80 bg-white rounded-xl p-4 shadow">Slide 3</div>
</div>
```

---

## RESPONSIVE PREFIXES

```
sm:   (640px+)
md:   (768px+)
lg:   (1024px+)
xl:   (1280px+)
2xl:  (1536px+)
```

> **Mobile-first:** Write the base style for mobile, then add `sm:` / `md:` / `lg:` overrides for larger screens.

**Examples:**
```html
<!-- Responsive grid -->
<div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">

<!-- Responsive text -->
<h1 class="text-2xl sm:text-3xl md:text-4xl lg:text-5xl font-bold">Responsive heading</h1>

<!-- Show/hide per breakpoint -->
<div class="hidden md:block">Only on desktop</div>
<div class="block md:hidden">Only on mobile</div>

<!-- Stack on mobile, row on desktop -->
<div class="flex flex-col md:flex-row gap-4">
  <aside class="w-full md:w-64">Sidebar</aside>
  <main class="flex-1">Content</main>
</div>

<!-- Responsive padding -->
<div class="px-4 sm:px-6 lg:px-8 py-8 sm:py-12 lg:py-16">Section</div>

<!-- Font size ramp -->
<p class="text-sm md:text-base lg:text-lg leading-relaxed">Body text</p>
```

---

## STATE VARIANTS

```
hover:bg-blue-600       focus:ring-2         active:scale-95
focus-within:border-blue-500
focus-visible:outline-2
disabled:opacity-50      disabled:cursor-not-allowed
checked:bg-blue-500
placeholder:text-gray-400
first:pt-0               last:pb-0           odd:bg-gray-50    even:bg-white
group-hover:text-blue-500   (parent has class="group")
peer-checked:text-blue-500  (sibling has class="peer")
```

**Examples:**
```html
<!-- Full interactive button -->
<button class="bg-blue-600 text-white px-4 py-2 rounded-lg font-medium
               hover:bg-blue-700
               focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-2
               active:scale-95
               disabled:opacity-50 disabled:cursor-not-allowed
               transition-all duration-150">
  Submit
</button>

<!-- Table with alternating rows -->
<tr class="odd:bg-white even:bg-gray-50 hover:bg-blue-50 transition-colors">

<!-- List without extra padding on first/last items -->
<ul>
  <li class="py-3 first:pt-0 last:pb-0 border-b last:border-0">Item</li>
</ul>

<!-- Group hover — child reacts to parent hover -->
<div class="group flex items-center gap-3 p-3 rounded-lg hover:bg-gray-50 cursor-pointer">
  <img class="w-10 h-10 rounded-full" />
  <div>
    <p class="font-medium group-hover:text-blue-600 transition-colors">Username</p>
    <p class="text-sm text-gray-500">@handle</p>
  </div>
  <svg class="ml-auto opacity-0 group-hover:opacity-100 transition-opacity text-gray-400">
    <!-- chevron -->
  </svg>
</div>

<!-- Peer — custom checkbox label -->
<label class="flex items-center gap-2 cursor-pointer">
  <input type="checkbox" class="peer sr-only" />
  <div class="w-10 h-6 rounded-full bg-gray-300 peer-checked:bg-blue-600 transition-colors"></div>
  Toggle
</label>

<!-- Focus within — highlight form field wrapper -->
<div class="border border-gray-300 rounded-lg focus-within:border-blue-500 focus-within:ring-2 focus-within:ring-blue-500/30 transition-all">
  <input class="outline-none px-3 py-2 w-full rounded-lg" placeholder="Search..." />
</div>

<!-- Placeholder styling -->
<input class="placeholder:text-gray-400 placeholder:italic px-3 py-2 border rounded-lg"
       placeholder="Enter your email" />
```

---

## DARK MODE

```css
@import "tailwindcss";
```

```html
<!-- Toggle with class on <html> -->
<html class="dark">
```

**Examples:**
```html
<!-- Page background -->
<body class="bg-white dark:bg-gray-950 text-gray-900 dark:text-gray-100">

<!-- Card -->
<div class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl p-5 shadow-sm dark:shadow-none">
  <h2 class="text-gray-900 dark:text-white font-semibold">Card title</h2>
  <p class="text-gray-500 dark:text-gray-400 text-sm mt-1">Description</p>
</div>

<!-- Input -->
<input class="bg-white dark:bg-gray-900 text-gray-900 dark:text-white
              border border-gray-300 dark:border-gray-600
              placeholder:text-gray-400 dark:placeholder:text-gray-500
              rounded-lg px-3 py-2 focus:ring-2 focus:ring-blue-500 outline-none" />

<!-- Button -->
<button class="bg-blue-600 dark:bg-blue-500 text-white
               hover:bg-blue-700 dark:hover:bg-blue-400
               px-4 py-2 rounded-lg">
  Submit
</button>

<!-- Navigation sidebar -->
<nav class="bg-gray-900 dark:bg-gray-950 text-gray-300 dark:text-gray-400 p-4">
  <a class="hover:text-white dark:hover:text-white hover:bg-gray-800 dark:hover:bg-gray-800
            block px-3 py-2 rounded-lg transition-colors">
    Dashboard
  </a>
</nav>

<!-- Code block -->
<pre class="bg-gray-100 dark:bg-gray-800 text-gray-800 dark:text-gray-200 rounded-lg p-4 text-sm font-mono overflow-x-auto">
  code here
</pre>
```

---

## ARBITRARY VALUES

```html
<!-- Sizing -->
w-[350px]           h-[calc(100vh-4rem)]       w-[calc(100%-2rem)]

<!-- Spacing -->
p-[1.25rem]        mt-[17px]                   gap-[1.375rem]

<!-- Typography -->
text-[1.375rem]    leading-[1.8]               tracking-[0.05em]

<!-- Colors -->
bg-[#1a1a2e]       text-[rgb(100,200,50)]      border-[#e5e7eb]

<!-- Position -->
top-[117px]        left-[calc(50%-11rem)]

<!-- Borders -->
border-[3px]       rounded-[10px]

<!-- Other -->
z-[100]            opacity-[0.35]              duration-[400ms]
bg-[radial-gradient(circle_at_top,#3b82f6,transparent_70%)]
grid-cols-[1fr_2fr_1fr]
shadow-[0_4px_24px_rgba(0,0,0,0.1)]
```

> **When to use arbitrary values:** For one-off pixel-precise values that don't fit the design system. If you're using the same arbitrary value in multiple places, add it to `@theme` instead.

---

## COMPONENT PATTERNS

### Button variants

```html
<!-- Primary -->
<button class="bg-blue-600 hover:bg-blue-700 text-white font-medium px-4 py-2 rounded-lg
               active:scale-95 transition-all duration-150 focus-visible:ring-2
               focus-visible:ring-blue-500 focus-visible:ring-offset-2 disabled:opacity-50">
  Primary
</button>

<!-- Secondary / outline -->
<button class="border border-gray-300 hover:border-gray-400 text-gray-700 bg-white
               hover:bg-gray-50 font-medium px-4 py-2 rounded-lg transition-all duration-150">
  Secondary
</button>

<!-- Ghost -->
<button class="text-gray-600 hover:text-gray-900 hover:bg-gray-100 font-medium px-4 py-2 rounded-lg transition-colors">
  Ghost
</button>

<!-- Destructive -->
<button class="bg-red-600 hover:bg-red-700 text-white font-medium px-4 py-2 rounded-lg transition-colors">
  Delete
</button>
```

### Input field

```html
<div class="space-y-1.5">
  <label class="block text-sm font-medium text-gray-700">Email</label>
  <input
    type="email"
    placeholder="you@example.com"
    class="w-full bg-white border border-gray-300 rounded-lg px-3 py-2 text-sm
           placeholder:text-gray-400
           focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent
           disabled:bg-gray-50 disabled:text-gray-500 disabled:cursor-not-allowed
           transition-shadow"
  />
  <p class="text-xs text-red-500">Error message here</p>
</div>
```

### Card

```html
<div class="bg-white rounded-2xl border border-gray-200 shadow-sm overflow-hidden
            hover:shadow-md transition-shadow duration-200">
  <img class="w-full h-48 object-cover" src="..." />
  <div class="p-5">
    <span class="text-xs font-medium uppercase tracking-wider text-blue-600">Category</span>
    <h3 class="mt-1 text-lg font-semibold text-gray-900 line-clamp-2">Card title here</h3>
    <p class="mt-2 text-sm text-gray-500 line-clamp-3">Description text...</p>
    <div class="mt-4 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <img class="w-6 h-6 rounded-full" src="..." />
        <span class="text-sm text-gray-600">Author Name</span>
      </div>
      <span class="text-xs text-gray-400">Mar 23</span>
    </div>
  </div>
</div>
```

### Alert / Toast

```html
<!-- Info -->
<div class="flex gap-3 bg-blue-50 border border-blue-200 rounded-lg p-4">
  <svg class="w-5 h-5 text-blue-500 shrink-0 mt-0.5">...</svg>
  <div>
    <p class="text-sm font-medium text-blue-800">Note</p>
    <p class="text-sm text-blue-700 mt-0.5">This is an informational message.</p>
  </div>
</div>

<!-- Error -->
<div class="flex gap-3 bg-red-50 border border-red-200 rounded-lg p-4">
  <svg class="w-5 h-5 text-red-500 shrink-0 mt-0.5">...</svg>
  <p class="text-sm text-red-700">Something went wrong. Please try again.</p>
</div>
```

### Avatar group

```html
<div class="flex -space-x-2">
  <img class="w-8 h-8 rounded-full border-2 border-white" src="..." />
  <img class="w-8 h-8 rounded-full border-2 border-white" src="..." />
  <img class="w-8 h-8 rounded-full border-2 border-white" src="..." />
  <div class="w-8 h-8 rounded-full border-2 border-white bg-gray-200 flex items-center justify-center text-xs font-medium text-gray-600">
    +4
  </div>
</div>
```

### Empty state

```html
<div class="flex flex-col items-center justify-center py-16 text-center">
  <div class="w-16 h-16 bg-gray-100 rounded-full flex items-center justify-center mb-4">
    <svg class="w-8 h-8 text-gray-400">...</svg>
  </div>
  <h3 class="text-base font-semibold text-gray-900">No results found</h3>
  <p class="mt-1 text-sm text-gray-500 max-w-xs">Try adjusting your search or filter.</p>
  <button class="mt-4 bg-blue-600 text-white px-4 py-2 rounded-lg text-sm font-medium hover:bg-blue-700 transition-colors">
    Clear filters
  </button>
</div>
```

---

## v3 → v4 CLASS CHANGES

| v3 | v4 |
|----|----|
| `shadow-sm` | `shadow-xs` |
| `shadow` | `shadow-sm` |
| `shadow-md` | `shadow-md` ✓ |
| `rounded-sm` | `rounded-xs` |
| `rounded` | `rounded-sm` |
| `blur-sm` | `blur-xs` |
| `blur` | `blur-sm` |
| `ring` | `ring-3` |
| `outline-none` | `outline-hidden` |
| `flex-shrink` | `shrink` |
| `flex-grow` | `grow` |
| `bg-opacity-50` | `bg-black/50` |
| `text-opacity-75` | `text-white/75` |
| `border-opacity-50` | `border-gray-500/50` |
| `overflow-ellipsis` | `text-ellipsis` |
| `decoration-slice` | `box-decoration-slice` |

Upgrade: `bunx @tailwindcss/upgrade`

---

## Resources

- [Docs](https://tailwindcss.com/docs)
- [v4 Overview](https://tailwindcss.com/blog/tailwindcss-v4)
- [Interactive Cheat Sheet](https://nerdcave.com/tailwind-cheat-sheet)
- [Tailwind UI](https://tailwindui.com) (paid components)
- [Heroicons](https://heroicons.com) (free SVG icons made for Tailwind)
- [shadcn/ui](https://ui.shadcn.com) (copy-paste component library)
EOF"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_typescript-eslint.md".into(),
                content: r###"# typescript-eslint — minikit docs
> **Version**: 8.57.2 · **Runtime**: bun · **npm**: https://www.npmjs.com/package/typescript-eslint

Tooling which enables you to use TypeScript with ESLint
**Homepage**: https://typescript-eslint.io/packages/typescript-eslint

---

> ✏️ Edit this file freely — it lives in ~/.minikit/docs/bun/typescript-eslint.md
> Every project you add typescript-eslint to will get YOUR edited version.
> To regenerate from original: `minicode skill reset typescript-eslint --runtime bun`

## My Notes

<!-- Add your own notes, snippets, team conventions here -->

---

## Installation

```json
{ "dependencies": { "typescript-eslint": "^8.57.2" } }
```

## Import

```typescript
import ... from 'typescript-eslint';
```

## Quick Start

```typescript
// Add your usage example here
```

## Links

- npm: https://www.npmjs.com/package/typescript-eslint
"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_typescript.md".into(),
                content: r###"# typescript

> Typed superset of JavaScript
> **Version:** 5.9.3 · **Runtime:** bun · [npm](https://www.npmjs.com/package/typescript) · [Docs](https://www.typescriptlang.org)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/typescript.md`
> To regenerate from original: `minicode skill reset typescript --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add -d typescript
bunx tsc --init   # generates tsconfig.json
```

> **Note:** Bun runs TypeScript natively — you don't need `ts-node` or compilation for running scripts. TypeScript here is for type checking and editor support.

---

## `tsconfig.json` (recommended for Bun projects)

```json
{
  "compilerOptions": {
    "target": "ES2023",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2023"],

    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,

    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowImportingTsExtensions": true,
    "noEmit": true,

    "outDir": "./dist",
    "rootDir": "./src",
    "baseUrl": ".",
    "paths": {
      "@/*": ["./src/*"]
    }
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}
```

**Key options:**
- `strict: true` — enables all strict checks (required by many libraries)
- `noEmit: true` — type-check only, let Bun handle execution
- `moduleResolution: "bundler"` — correct for Bun and Vite
- `noUncheckedIndexedAccess` — array/object access returns `T | undefined`

---

## Type Checking

```bash
# Type-check without emitting files
bunx tsc --noEmit

# Watch mode
bunx tsc --noEmit --watch
```

---

## Core Types

```ts
// Primitives
let name: string = 'Jane'
let age: number = 30
let active: boolean = true
let id: bigint = 9007199254740991n
let sym: symbol = Symbol('id')

// Arrays
let tags: string[] = ['a', 'b']
let tags: Array<string> = ['a', 'b']

// Tuple
let coords: [number, number] = [0, 0]

// Object
let user: { name: string; age?: number } = { name: 'Jane' }

// Union
let value: string | number
let status: 'active' | 'inactive' | 'pending'

// Intersection
type AdminUser = User & { role: 'admin' }

// Any / Unknown / Never
let anything: any
let safe: unknown
function fail(): never { throw new Error('') }

// Null / Undefined
let maybe: string | null = null
let optional: string | undefined
```

---

## Interfaces & Types

```ts
interface User {
  id: number
  email: string
  name?: string           // optional
  readonly createdAt: Date
}

type Status = 'active' | 'inactive'

type ID = string | number

// Extending
interface Admin extends User {
  role: 'admin'
  permissions: string[]
}

// Merging (interface only)
interface Window {
  myCustomProp: string
}
```

---

## Generics

```ts
function identity<T>(value: T): T {
  return value
}

function first<T>(arr: T[]): T | undefined {
  return arr[0]
}

interface ApiResponse<T> {
  data: T
  error: string | null
  status: number
}

// Constraints
function getLength<T extends { length: number }>(item: T): number {
  return item.length
}

// Default generic
type Pagination<T = unknown> = {
  items: T[]
  total: number
  page: number
}
```

---

## Utility Types

```ts
// Make all properties optional
type PartialUser = Partial<User>

// Make all properties required
type RequiredUser = Required<User>

// Make all properties read-only
type ReadonlyUser = Readonly<User>

// Pick specific properties
type PublicUser = Pick<User, 'id' | 'name'>

// Omit specific properties
type UserWithoutPassword = Omit<User, 'password'>

// Record — typed key-value map
type UserMap = Record<string, User>

// Extract / Exclude from union
type StringOrNumber = string | number | boolean
type OnlyStrings = Extract<StringOrNumber, string>    // string
type NoStrings = Exclude<StringOrNumber, string>      // number | boolean

// Return type of a function
type Result = ReturnType<typeof fetchUser>

// Parameters of a function
type Params = Parameters<typeof fetchUser>

// Infer from Promise
type Resolved = Awaited<Promise<User>>

// Non-nullable
type NonNull = NonNullable<string | null | undefined>  // string
```

---

## Type Narrowing

```ts
function process(value: string | number) {
  if (typeof value === 'string') {
    return value.toUpperCase()  // string here
  }
  return value.toFixed(2)       // number here
}

// instanceof
if (error instanceof Error) { error.message }

// in operator
if ('email' in user) { user.email }

// Type predicates
function isUser(obj: unknown): obj is User {
  return typeof obj === 'object' && obj !== null && 'email' in obj
}

// Assertion functions
function assertDefined<T>(val: T | null | undefined): asserts val is T {
  if (val == null) throw new Error('Expected defined value')
}
```

---

## `satisfies` Operator

```ts
const config = {
  port: 3000,
  host: 'localhost',
} satisfies Record<string, string | number>

config.port  // inferred as number, not string | number
```

---

## `as const`

```ts
const ROLES = ['admin', 'user', 'guest'] as const
type Role = typeof ROLES[number]  // 'admin' | 'user' | 'guest'

const config = { port: 3000, host: 'localhost' } as const
// All properties are readonly literal types
```

---

## Resources

- [TypeScript Docs](https://www.typescriptlang.org/docs)
- [tsconfig Reference](https://www.typescriptlang.org/tsconfig)
- [Utility Types](https://www.typescriptlang.org/docs/handbook/utility-types.html)
- [TypeScript Playground](https://www.typescriptlang.org/play)"###.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_vite.md".into(),
                content: r####"# vite

> Next-generation frontend build tool — instant dev server, fast HMR, optimized builds
> **Version:** 8.0.1 · **Runtime:** bun · [npm](https://www.npmjs.com/package/vite) · [Docs](https://vite.dev)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/vite.md`
> To regenerate from original: `minicode skill reset vite --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
# Scaffold a new project
bun create vite my-app --template react-ts

# Add to existing project
bun add -d vite @vitejs/plugin-react
```

## Templates

```bash
bun create vite my-app --template react-ts      # React + TypeScript
bun create vite my-app --template react         # React + JS
bun create vite my-app --template vue-ts
bun create vite my-app --template svelte-ts
bun create vite my-app --template vanilla-ts
```

---

## Config (`vite.config.ts`)

```ts
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig({
  plugins: [react()],

  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },

  server: {
    port: 3000,
    open: true,               // auto-open browser
    proxy: {
      '/api': {
        target: 'http://localhost:8000',
        changeOrigin: true,
      },
    },
  },

  build: {
    outDir: 'dist',
    sourcemap: true,
    minify: 'esbuild',        // 'esbuild' | 'terser' | false
    target: 'es2020',
    chunkSizeWarningLimit: 1000, // KB
  },

  preview: {
    port: 4173,
  },
})
```

---

## CLI Commands

```bash
bun run dev        # start dev server (HMR enabled)
bun run build      # production build → dist/
bun run preview    # preview production build locally
bun run vite       # same as dev
```

---

## Environment Variables

Vite exposes env vars prefixed with `VITE_` to client code:

```bash
# .env
VITE_API_URL=https://api.example.com
VITE_APP_NAME=MyApp
```

```ts
// Access in code
const apiUrl = import.meta.env.VITE_API_URL
const isDev = import.meta.env.DEV          // boolean
const isProd = import.meta.env.PROD        // boolean
const mode = import.meta.env.MODE         // 'development' | 'production'
```

### Multiple `.env` files

```
.env                  # always loaded
.env.local            # always loaded, gitignored
.env.development      # loaded in dev mode
.env.production       # loaded in build mode
.env.production.local # loaded in build mode, gitignored
```

---

## Path Aliases (`@/`)

```ts
// vite.config.ts
resolve: {
  alias: {
    '@': fileURLToPath(new URL('./src', import.meta.url)),
  },
},
```

```json
// tsconfig.json — must match
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*": ["./src/*"]
    }
  }
}
```

```ts
// Usage
import { Button } from '@/components/Button'
import { api } from '@/lib/api'
```

---

## Plugins

```bash
bun add -d @vitejs/plugin-react          # React + Fast Refresh
bun add -d @vitejs/plugin-react-swc      # React + SWC (faster)
bun add -d @tailwindcss/vite             # Tailwind v4
bun add -d vite-plugin-svgr              # import SVG as React components
bun add -d vite-tsconfig-paths           # auto-read tsconfig paths
```

```ts
import react from '@vitejs/plugin-react'
import svgr from 'vite-plugin-svgr'
import tsconfigPaths from 'vite-tsconfig-paths'
import tailwindcss from '@tailwindcss/vite'

plugins: [react(), svgr(), tsconfigPaths(), tailwindcss()]
```

---

## Static Assets

```ts
// Import image — returns URL string
import logo from './assets/logo.png'
<img src={logo} />

// Raw file content
import readme from './README.md?raw'

// SVG as component (requires vite-plugin-svgr)
import Logo from './logo.svg?react'
<Logo className="w-8 h-8" />

// Public folder — served at root, never processed
// Put in /public/ → reference as /logo.png
```

---

## Build Output

```bash
dist/
├── index.html
├── assets/
│   ├── index-[hash].js      # main bundle
│   ├── vendor-[hash].js     # auto code-split chunk
│   └── index-[hash].css
```

### Manual chunks

```ts
build: {
  rollupOptions: {
    output: {
      manualChunks: {
        'react-vendor': ['react', 'react-dom'],
        'query': ['@tanstack/react-query'],
      },
    },
  },
},
```

---

## Resources

- [Vite Docs](https://vite.dev/guide/)
- [Config Reference](https://vite.dev/config/)
- [Plugins](https://vite.dev/plugins/)
- [GitHub](https://github.com/vitejs/vite)"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_zod.md".into(),
                content: r####"# zod

> TypeScript-first schema validation with static type inference
> **Version:** 4.3.6 · **Runtime:** bun · [npm](https://www.npmjs.com/package/zod) · [Docs](https://zod.dev)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/zod.md`
> To regenerate from original: `minicode skill reset zod --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add zod
```

Requires `strict: true` in `tsconfig.json`.

## Import

```ts
import * as z from 'zod'
// or
import { z } from 'zod'
```

> **Zod v4 import:** Use `import { z } from 'zod'` — the named export is new in v4 and preferred.

---

## Core idea

Write the schema once — get validation **and** the TypeScript type for free:

```ts
import { z } from 'zod'

const UserSchema = z.object({
  id: z.number(),
  email: z.string().email(),
  name: z.string().optional(),
})

type User = z.infer<typeof UserSchema>
// { id: number; email: string; name?: string }

// Validate at runtime
const user = UserSchema.parse(untrustedData)      // throws on failure
const result = UserSchema.safeParse(untrustedData) // never throws
```

---

## Parse vs SafeParse

```ts
// parse — throws ZodError if invalid
try {
  const user = UserSchema.parse(data)
} catch (err) {
  if (err instanceof z.ZodError) {
    console.log(err.issues)
  }
}

// safeParse — always returns a result object
const result = UserSchema.safeParse(data)

if (result.success) {
  console.log(result.data)     // typed and validated
} else {
  console.log(result.error.issues)       // array of issues
  console.log(result.error.flatten())    // { fieldErrors, formErrors }
  console.log(result.error.format())     // nested object of errors
}

// Async variants (for async refinements)
const result = await UserSchema.safeParseAsync(data)
```

---

## Primitive Types

```ts
z.string()
z.number()
z.boolean()
z.bigint()
z.date()
z.symbol()
z.undefined()
z.null()
z.void()
z.any()
z.unknown()
z.never()
```

---

## String Validation

```ts
z.string()
  .min(3, 'At least 3 characters')
  .max(100, 'Too long')
  .length(10, 'Exactly 10 characters')
  .email('Invalid email')
  .url('Invalid URL')
  .uuid('Invalid UUID')
  .cuid()
  .cuid2()
  .nanoid()
  .regex(/^\d{4}$/, 'Must be 4 digits')
  .startsWith('https://')
  .endsWith('.com')
  .includes('@')
  .toLowerCase()           // transforms to lowercase
  .toUpperCase()
  .trim()                  // strips whitespace
  .nonempty('Required')    // disallow empty string ''
```

**Common patterns:**
```ts
const emailSchema = z.string().trim().toLowerCase().email()
const slugSchema  = z.string().regex(/^[a-z0-9-]+$/, 'Invalid slug')
const urlSchema   = z.string().url()
const uuidSchema  = z.string().uuid()
const phoneSchema = z.string().regex(/^\+?[\d\s\-()]{7,15}$/)
```

---

## Number Validation

```ts
z.number()
  .int('Must be an integer')
  .positive('Must be positive')
  .negative()
  .nonnegative()
  .nonpositive()
  .gt(0)     .gte(1)    // greater than / greater than or equal
  .lt(100)   .lte(99)   // less than / less than or equal
  .multipleOf(5)
  .safe()               // within Number.MAX_SAFE_INTEGER
  .finite()             // not Infinity
```

**Coerce strings from HTML inputs:**
```ts
// HTML <input type="number"> returns a string — coerce it
const ageSchema = z.coerce.number().int().positive().max(120)
const priceSchema = z.coerce.number().multipleOf(0.01).nonnegative()
```

---

## Object

```ts
const UserSchema = z.object({
  id: z.number(),
  email: z.string().email(),
  name: z.string().optional(),
  role: z.enum(['admin', 'user', 'guest']),
  createdAt: z.date().optional(),
})

// Extend — add fields
const AdminSchema = UserSchema.extend({
  permissions: z.array(z.string()),
})

// Merge two schemas
const MergedSchema = SchemaA.merge(SchemaB)

// Pick — keep only these fields
const PublicUserSchema = UserSchema.pick({ id: true, name: true, email: true })

// Omit — remove these fields
const CreateUserSchema = UserSchema.omit({ id: true, createdAt: true })

// Partial — make all fields optional (great for PATCH endpoints)
const UpdateUserSchema = UserSchema.partial()

// Required — make all fields required
const RequiredSchema = UserSchema.required()

// Passthrough — allow unknown keys (don't strip them)
UserSchema.passthrough()

// Strict — reject unknown keys
z.strictObject({ name: z.string() })

// Strip (default) — remove unknown keys silently
UserSchema.strip()
```

---

## Array

```ts
z.array(z.string())
z.array(z.number()).nonempty('At least one item required')
z.array(z.string()).min(1).max(10)
z.array(z.string()).length(3)

// Typed tuple
const CoordSchema = z.tuple([z.number(), z.number()])
type Coord = z.infer<typeof CoordSchema> // [number, number]

// Rest element
const ArgsSchema = z.tuple([z.string()]).rest(z.number())
// [string, ...number[]]
```

---

## Enum

```ts
// Zod enum (most common)
const RoleSchema = z.enum(['admin', 'user', 'guest'])
type Role = z.infer<typeof RoleSchema> // 'admin' | 'user' | 'guest'

// Access values
RoleSchema.options  // ['admin', 'user', 'guest']
RoleSchema.enum.admin // 'admin'

// From a const array (keeps type narrow)
const ROLES = ['admin', 'user', 'guest'] as const
const RoleSchema = z.enum(ROLES)

// Native TypeScript enum
enum Status { Active, Inactive }
z.nativeEnum(Status)
```

---

## Union & Intersection

```ts
// Union
const IdSchema = z.union([z.string(), z.number()])
const IdSchema = z.string().or(z.number())  // shorthand

// Discriminated union — faster and better errors
const ShapeSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('circle'),    radius: z.number() }),
  z.object({ type: z.literal('rect'),      width: z.number(), height: z.number() }),
  z.object({ type: z.literal('triangle'),  base: z.number(), height: z.number() }),
])

// Intersection
const AdminSchema = UserSchema.and(z.object({ adminKey: z.string() }))
```

---

## Optional / Nullable / Default

```ts
z.string().optional()          // string | undefined
z.string().nullable()          // string | null
z.string().nullish()           // string | null | undefined
z.string().default('fallback') // uses 'fallback' when input is undefined
z.string().catch('fallback')   // uses 'fallback' on validation failure (never throws)
```

---

## Literal

```ts
z.literal('admin')     // exactly 'admin'
z.literal(42)
z.literal(true)

// Multiple literals
const StatusSchema = z.union([
  z.literal('active'),
  z.literal('inactive'),
  z.literal('banned'),
])
```

---

## Transform

Transforms run **after** validation and can change the output type.

```ts
// String → number
const NumberFromString = z.string()
  .transform((val) => parseInt(val, 10))
  .pipe(z.number().int())  // validate the result too

// Object → different shape
const ApiUserSchema = z.object({ first_name: z.string(), last_name: z.string() })
  .transform(({ first_name, last_name }) => ({
    name: `${first_name} ${last_name}`,
  }))

// Trim and lowercase email
const emailSchema = z.string()
  .transform((s) => s.trim().toLowerCase())
  .pipe(z.string().email())

// Input vs output types differ after transform
type Input  = z.input<typeof NumberFromString>   // string
type Output = z.output<typeof NumberFromString>  // number
// same as z.infer<> which gives the output
```

---

## Refinements (custom validation)

```ts
// Single refinement
const PasswordSchema = z.string()
  .min(8)
  .refine((val) => /[A-Z]/.test(val), { message: 'Need one uppercase letter' })
  .refine((val) => /[0-9]/.test(val), { message: 'Need one number' })

// Cross-field validation
const SignupSchema = z.object({
  password: z.string().min(8),
  confirm:  z.string(),
}).refine((data) => data.password === data.confirm, {
  message: 'Passwords do not match',
  path: ['confirm'],    // attach error to this field
})

// Async refinement (e.g. check DB)
const EmailSchema = z.string().email().superRefine(async (email, ctx) => {
  const exists = await checkEmailInDb(email)
  if (exists) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      message: 'Email already in use',
    })
  }
})
```

---

## Record & Map

```ts
// Record — typed key-value object
const CacheSchema = z.record(z.string(), z.number())
type Cache = z.infer<typeof CacheSchema> // Record<string, number>

// Record with enum keys
const ScoreSchema = z.record(z.enum(['win', 'loss', 'draw']), z.number())

// Map
const MapSchema = z.map(z.string(), z.number())
// Map<string, number>

// Set
const TagSetSchema = z.set(z.string())
// Set<string>
```

---

## Error Handling in Detail

```ts
const result = UserSchema.safeParse(data)

if (!result.success) {
  // Flat — great for forms
  const flat = result.error.flatten()
  // {
  //   formErrors: [],          // top-level errors
  //   fieldErrors: {           // per-field errors
  //     email: ['Invalid email'],
  //     name: ['Too short'],
  //   }
  // }

  // Format — nested object
  const formatted = result.error.format()
  // { email: { _errors: ['Invalid email'] }, name: { _errors: [...] } }

  // Raw issues array
  result.error.issues.forEach((issue) => {
    console.log(issue.path)    // ['email']
    console.log(issue.message) // 'Invalid email'
    console.log(issue.code)    // 'invalid_string'
  })
}
```

---

## Real-world Patterns

### API request body validation (with Hono)

```ts
import { z } from 'zod'

const CreatePostSchema = z.object({
  title:     z.string().min(1).max(200).trim(),
  content:   z.string().min(1),
  tags:      z.array(z.string()).max(5).default([]),
  published: z.boolean().default(false),
})

app.post('/posts', async (c) => {
  const result = CreatePostSchema.safeParse(await c.req.json())
  if (!result.success) {
    return c.json({ errors: result.error.flatten().fieldErrors }, 400)
  }
  const post = await db.createPost(result.data)
  return c.json(post, 201)
})
```

### Environment variable validation

```ts
const EnvSchema = z.object({
  DATABASE_URL: z.string().url(),
  PORT:         z.coerce.number().default(3000),
  NODE_ENV:     z.enum(['development', 'production', 'test']).default('development'),
  JWT_SECRET:   z.string().min(32),
})

export const env = EnvSchema.parse(process.env)
// Now env.PORT is a number, not a string
```

### Shared schema between server and client

```ts
// shared/schemas.ts
export const UserSchema = z.object({
  id:    z.string().uuid(),
  email: z.string().email(),
  name:  z.string(),
  role:  z.enum(['admin', 'user']),
})

export const CreateUserSchema = UserSchema.omit({ id: true })
export const UpdateUserSchema = UserSchema.partial().omit({ id: true })

export type User       = z.infer<typeof UserSchema>
export type CreateUser = z.infer<typeof CreateUserSchema>
export type UpdateUser = z.infer<typeof UpdateUserSchema>
```

### PATCH endpoint (partial update)

```ts
const UpdateUserSchema = z.object({
  name:  z.string().min(1).optional(),
  email: z.string().email().optional(),
  role:  z.enum(['admin', 'user']).optional(),
}).refine(
  (data) => Object.keys(data).length > 0,
  { message: 'At least one field required' }
)
```

---

## Resources

- [Zod Docs](https://zod.dev)
- [GitHub](https://github.com/colinhacks/zod)
- [v4 Migration Guide](https://zod.dev/v4)
- [Zod Playground](https://zod.dev/play)"####.into(),
                binary_content: None,
            },
            StackFile {
                path: "minikit_docs/packages/minikit_zustand.md".into(),
                content: r####"# zustand

> Minimal, fast state management for React
> **Version:** 5.0.12 · **Runtime:** bun · [npm](https://www.npmjs.com/package/zustand) · [Docs](https://zustand.docs.pmnd.rs)

---

> ✏️ **This file is yours to edit** — it lives at `~/.minikit/docs/bun/zustand.md`
> To regenerate from original: `minicode skill reset zustand --runtime bun`

---

## My Notes

<!-- Add your own notes, snippets, and team conventions here -->

---

## Install

```bash
bun add zustand
```

---

## Usage

### Create a Store

```ts
import { create } from 'zustand'

interface BearState {
  bears: number
  increase: () => void
  reset: () => void
}

const useBearStore = create<BearState>((set) => ({
  bears: 0,
  increase: () => set((state) => ({ bears: state.bears + 1 })),
  reset: () => set({ bears: 0 }),
}))
```

### Use in Components

```tsx
function BearCount() {
  const bears = useBearStore((state) => state.bears)
  return <p>{bears} bears</p>
}

function Controls() {
  const increase = useBearStore((state) => state.increase)
  return <button onClick={increase}>+1</button>
}
```

> No providers needed — works anywhere in the tree.

---

## Selecting State

```ts
// Single value — re-renders only when this value changes
const bears = useBearStore((state) => state.bears)

// Multiple values — use useShallow to avoid unnecessary re-renders
import { useShallow } from 'zustand/react/shallow'

const { bears, fish } = useBearStore(
  useShallow((state) => ({ bears: state.bears, fish: state.fish }))
)

// Fetch all state (re-renders on any change — use sparingly)
const state = useBearStore()
```

---

## Updating State

```ts
// Merge update (shallow merge — does NOT deep merge)
set({ bears: 5 })

// Functional update — read current state
set((state) => ({ bears: state.bears + 1 }))

// Replace entire state (second arg = true)
set({}, true)

// Read state outside of set with get
const useStore = create<State>((set, get) => ({
  count: 0,
  double: () => {
    const current = get().count
    set({ count: current * 2 })
  },
}))
```

---

## Async Actions

```ts
const useStore = create<State>((set) => ({
  users: [],
  loading: false,
  fetchUsers: async () => {
    set({ loading: true })
    const users = await api.getUsers()
    set({ users, loading: false })
  },
}))
```

---

## Outside React

```ts
// Read state without a hook
const bears = useBearStore.getState().bears

// Write state
useBearStore.setState({ bears: 10 })

// Subscribe to all changes
const unsub = useBearStore.subscribe((state) => {
  console.log('new state:', state.bears)
})
unsub() // unsubscribe
```

---

## Middleware

### Persist (localStorage / sessionStorage)

```ts
import { create } from 'zustand'
import { persist, createJSONStorage } from 'zustand/middleware'

const useStore = create(
  persist<State>(
    (set) => ({
      token: null,
      setToken: (token) => set({ token }),
    }),
    {
      name: 'auth-storage',                               // localStorage key
      storage: createJSONStorage(() => sessionStorage),   // default: localStorage
    }
  )
)
```

### Immer (nested state mutations)

```ts
import { immer } from 'zustand/middleware/immer'

const useStore = create(
  immer<State>((set) => ({
    user: { profile: { name: 'Jane' } },
    updateName: (name: string) =>
      set((state) => {
        state.user.profile.name = name  // mutate directly
      }),
  }))
)
```

### DevTools (Redux DevTools extension)

```ts
import { devtools } from 'zustand/middleware'

const useStore = create(
  devtools<State>(
    (set) => ({ bears: 0, increase: () => set({ bears: 1 }) }),
    { name: 'BearStore' }
  )
)
```

### Combining Middleware

```ts
const useStore = create<State>()(
  devtools(
    persist(
      immer((set) => ({
        // your state
      })),
      { name: 'my-store' }
    )
  )
)
```

---

## Slices Pattern (large stores)

```ts
// bears.ts
export const createBearSlice = (set) => ({
  bears: 0,
  addBear: () => set((s) => ({ bears: s.bears + 1 })),
})

// fish.ts
export const createFishSlice = (set) => ({
  fish: 0,
  addFish: () => set((s) => ({ fish: s.fish + 1 })),
})

// store.ts
import { create } from 'zustand'

const useStore = create<BearSlice & FishSlice>((...args) => ({
  ...createBearSlice(...args),
  ...createFishSlice(...args),
}))
```

---

## Resources

- [Docs](https://zustand.docs.pmnd.rs)
- [GitHub](https://github.com/pmndrs/zustand)
- [Guides](https://zustand.docs.pmnd.rs/guides)"####.into(),
                binary_content: None,
            },
    ]
}

/// Extracts the canonical npm package name from a documentation file's `npmjs.com/package/<pkg>` link if present.
pub fn extract_doc_npm_package(content: &str) -> Option<&str> {
    const MARKER: &str = "npmjs.com/package/";
    let idx = content.find(MARKER)?;
    let rest = &content[idx + MARKER.len()..];
    let end = rest
        .find(|c: char| c.is_ascii_whitespace() || matches!(c, ')' | ']' | '"' | '\'' | '>'))
        .unwrap_or(rest.len());
    let pkg = rest[..end].trim_end_matches('/');
    if pkg.is_empty() {
        None
    } else {
        Some(pkg)
    }
}

/// Filters `react_package_docs()` so that only package documentation files corresponding to
/// dependencies actually declared in `package.json` within `files` are included in the stack.
pub fn react_package_docs_for_files(files: &[StackFile]) -> Vec<StackFile> {
    let mut declared_packages = std::collections::HashSet::new();
    let mut declared_filenames = std::collections::HashSet::new();

    for f in files {
        if f.path == "package.json" {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&f.content) {
                for field in &["dependencies", "devDependencies", "peerDependencies"] {
                    if let Some(obj) = json.get(*field).and_then(|v| v.as_object()) {
                        for key in obj.keys() {
                            let trimmed = key.trim().to_ascii_lowercase();
                            if !trimmed.is_empty() {
                                let slug = trimmed.replace(['@', '/'], "__");
                                declared_filenames.insert(format!("minikit_{}.md", slug));
                                declared_packages.insert(trimmed);
                            }
                        }
                    }
                }
            }
        }
    }

    if declared_filenames.is_empty() {
        return react_package_docs();
    }

    react_package_docs()
        .into_iter()
        .filter(|doc| {
            let by_filename = std::path::Path::new(&doc.path)
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| declared_filenames.contains(name));
            let by_npm_url = extract_doc_npm_package(&doc.content)
                .is_some_and(|pkg| declared_packages.contains(&pkg.to_ascii_lowercase()));
            by_filename || by_npm_url
        })
        .collect()
}
