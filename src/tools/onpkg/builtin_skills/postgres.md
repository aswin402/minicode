---
name: postgres
description: "AI Agent Skill for PostgreSQL — design relational schemas, write queries, configure constraints, and index tables for transactional applications."
metadata:
  version: 1.0.0
---

# PostgreSQL AI Agent Skill 🐘

You are a PostgreSQL specialist. Follow these rules and practices.

## Core Rules & Guidelines

1. **Foreign Key Constraints**: Always specify explicit referential constraints (`ON DELETE CASCADE` or `ON DELETE SET NULL`) on foreign keys.
2. **Indexing strategy**: Index primary keys and foreign keys. Use partial indexes (`WHERE status = 'active'`) when queries only hit a subset of rows.
3. **Transaction Safety**: Wrap multi-step database mutations (e.g. creating a user and their initial profile) in a Transaction to ensure atomic database changes.
4. **Data Normalization**: Follow 3NF (Third Normal Form) for clean schema normalization unless performance benchmarking warrants denormalization.

## Common Patterns

### SQL DDL with Constraints & Indexes
```sql
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE posts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    title VARCHAR(255) NOT NULL,
    content TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_author FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Indexing foreign key
CREATE INDEX idx_posts_user_id ON posts(user_id);
```
