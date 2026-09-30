# 🏛️ Architecture Documentation: worker2_backend

> REST backend API service for inventory and order management system.

## 📐 Clean Architecture Layer Breakdown

| Layer | File Count | Primary Components |
|:--- |:--- |:--- |
| **Routes** | 4 | health.routes, inventory.routes, orders.routes, metrics.routes |
| **Services** | 3 | inventory.service, orders.service, metrics.service |
| **Types** | 2 | index.ts (types), supertest.d.ts |
| **Utils** | 1 | validation.ts |
| **Tests** | 1 | api.test.ts |

## 🗺️ Project Structure

```
worker2_backend/
├── package.json
├── tsconfig.json
├── src/
│   ├── server.ts           # Main entry point
│   ├── routes/
│   │   ├── health.routes.ts      # GET /health
│   │   ├── inventory.routes.ts   # CRUD /api/inventory
│   │   ├── orders.routes.ts       # CRUD /api/orders
│   │   └── metrics.routes.ts     # GET /metrics
│   ├── services/
│   │   ├── inventory.service.ts  # Inventory business logic
│   │   ├── orders.service.ts     # Order business logic
│   │   └── metrics.service.ts   # Metrics collection
│   ├── types/
│   │   ├── index.ts              # TypeScript interfaces
│   │   └── supertest.d.ts        # Test type declarations
│   ├── utils/
│   │   └── validation.ts         # Request validation utilities
│   └── __tests__/
│       └── api.test.ts           # Integration tests
└── dist/                         # Compiled JavaScript output
```

## 🔌 API Endpoints

| Method | Endpoint | Description |
|:--- |:--- |:--- |
| GET | `/health` | Health check with uptime & memory stats |
| GET | `/api/inventory` | List all inventory items |
| GET | `/api/inventory/:id` | Get item by ID |
| POST | `/api/inventory` | Create new inventory item |
| PUT | `/api/inventory/:id` | Update inventory item |
| DELETE | `/api/inventory/:id` | Delete inventory item |
| GET | `/api/orders` | List all orders (optional `?status=` filter) |
| GET | `/api/orders/:id` | Get order by ID |
| POST | `/api/orders` | Create new order |
| PATCH | `/api/orders/:id/status` | Update order status |
| DELETE | `/api/orders/:id` | Delete order |
| GET | `/metrics` | Get JSON metrics |
| GET | `/metrics/prometheus` | Get Prometheus-format metrics |

## 🔑 Core Symbols

| Symbol | Type | File | Description |
|:--- |:--- |:--- |:--- |
| `app` | Express | server.ts | Main application instance |
| `createItem` | Function | inventory.service.ts | Create inventory item |
| `createOrder` | Function | orders.service.ts | Create new order |
| `getMetrics` | Function | metrics.service.ts | Retrieve system metrics |
| `validateCreateOrder` | Function | orders.routes.ts | Order validation |
| `validateCreateItem` | Function | inventory.routes.ts | Inventory validation |

## 📊 Technology Stack

- **Runtime:** Node.js v22+
- **Framework:** Express.js 4.x
- **Language:** TypeScript 5.x
- **Testing:** Node.js native test runner (node:test)
- **HTTP Client:** Supertest
