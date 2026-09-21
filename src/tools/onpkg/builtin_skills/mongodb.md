---
name: mongodb
description: "AI Agent Skill for MongoDB & Mongoose — design flexible document schemas, write aggregations, and optimize query indexes for Node.js backends."
metadata:
  version: 1.0.0
---

# MongoDB & Mongoose AI Agent Skill 🍃

You are a MongoDB/Mongoose specialist. Follow these rules and practices.

## Core Rules & Guidelines

1. **Schema Definition**: Always define schemas with strict validation rules. Use `timestamps: true` to automatically manage `createdAt` and `updatedAt`.
2. **Indexing**: Always create indexes for fields frequently queried (e.g. `email`, `status`, or compound keys) to optimize search lookups.
3. **Data Denormalization**: In document DBs, denormalize data when read patterns outweigh write patterns. Embed documents instead of linking them if the child data is logically part of the parent.
4. **Connection Handling**: Maintain a single database connection throughout the application lifecycle.

## Common Patterns

### Mongoose Schema & Indexing
```typescript
import mongoose, { Schema, Document } from 'mongoose';

export interface IUser extends Document {
  email: string;
  name: string;
  role: 'user' | 'admin';
  createdAt: Date;
  updatedAt: Date;
}

const UserSchema: Schema = new Schema(
  {
    email: { type: String, required: true, unique: true, lowercase: true, trim: true },
    name: { type: String, required: true },
    role: { type: String, enum: ['user', 'admin'], default: 'user' },
  },
  { timestamps: true }
);

// Indexes
UserSchema.index({ email: 1 });
UserSchema.index({ role: 1 });

export const User = mongoose.model<IUser>('User', UserSchema);
```
