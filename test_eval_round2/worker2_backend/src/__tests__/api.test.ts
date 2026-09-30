import { describe, it, beforeEach } from 'node:test';
import assert from 'node:assert';
import supertest from 'supertest';
import { app } from '../server.js';
import * as inventoryService from '../services/inventory.service.js';
import * as ordersService from '../services/orders.service.js';
import * as metricsService from '../services/metrics.service.js';

const request = supertest(app);

describe('Health Check Endpoint', () => {
  it('GET /health should return healthy status', async () => {
    const response = await request.get('/health');
    
    assert.strictEqual(response.status, 200);
    assert.strictEqual(response.body.success, true);
    assert.strictEqual(response.body.data.status, 'healthy');
    assert.ok(typeof response.body.data.uptime === 'number');
    assert.ok(response.body.data.memory);
    assert.ok(response.body.data.memory.used);
    assert.ok(response.body.data.memory.total);
    assert.ok(response.body.data.timestamp);
  });
});

describe('Inventory CRUD Endpoints', () => {
  let createdItemId: string;

  beforeEach(() => {
    inventoryService.clearInventory();
    inventoryService.initializeSampleData();
  });

  it('GET /api/inventory should return all inventory items', async () => {
    const response = await request.get('/api/inventory');
    
    assert.strictEqual(response.status, 200);
    assert.strictEqual(response.body.success, true);
    assert.ok(Array.isArray(response.body.data));
    assert.ok(response.body.data.length > 0);
  });

  it('GET /api/inventory/:id should return a specific item', async () => {
    const items = inventoryService.getAllItems();
    const itemId = items[0].id;
    
    const response = await request.get(`/api/inventory/${itemId}`);
    
    assert.strictEqual(response.status, 200);
    assert.strictEqual(response.body.success, true);
    assert.strictEqual(response.body.data.id, itemId);
  });

  it('GET /api/inventory/:id should return 404 for non-existent item', async () => {
    const response = await request.get('/api/inventory/non-existent-id');
    
    assert.strictEqual(response.status, 404);
    assert.strictEqual(response.body.success, false);
  });

  it('POST /api/inventory should create a new item', async () => {
    const newItem = {
      name: 'Test Product',
      description: 'A test product',
      quantity: 100,
      price: 49.99,
      category: 'test'
    };
    
    const response = await request
      .post('/api/inventory')
      .send(newItem);
    
    assert.strictEqual(response.status, 201);
    assert.strictEqual(response.body.success, true);
    assert.ok(response.body.data.id);
    assert.strictEqual(response.body.data.name, newItem.name);
    createdItemId = response.body.data.id;
  });

  it('POST /api/inventory should fail with invalid data', async () => {
    const invalidItem = {
      name: '',
      quantity: -1
    };
    
    const response = await request
      .post('/api/inventory')
      .send(invalidItem);
    
    assert.strictEqual(response.status, 400);
    assert.strictEqual(response.body.success, false);
  });

  it('PUT /api/inventory/:id should update an item', async () => {
    const items = inventoryService.getAllItems();
    const itemId = items[0].id;
    
    const response = await request
      .put(`/api/inventory/${itemId}`)
      .send({ name: 'Updated Name' });
    
    assert.strictEqual(response.status, 200);
    assert.strictEqual(response.body.success, true);
    assert.strictEqual(response.body.data.name, 'Updated Name');
  });

  it('DELETE /api/inventory/:id should delete an item', async () => {
    const items = inventoryService.getAllItems();
    const itemId = items[0].id;
    
    const response = await request.delete(`/api/inventory/${itemId}`);
    
    assert.strictEqual(response.status, 200);
    assert.strictEqual(response.body.success, true);
    
    // Verify deletion
    const getResponse = await request.get(`/api/inventory/${itemId}`);
    assert.strictEqual(getResponse.status, 404);
  });
});

describe('Orders Endpoints', () => {
  beforeEach(() => {
    inventoryService.clearInventory();
    ordersService.clearOrders();
    inventoryService.initializeSampleData();
  });

  it('GET /api/orders should return all orders', async () => {
    const response = await request.get('/api/orders');
    
    assert.strictEqual(response.status, 200);
    assert.strictEqual(response.body.success, true);
    assert.ok(Array.isArray(response.body.data));
  });

  it('GET /api/orders should filter by status', async () => {
    // Create an order first
    const items = inventoryService.getAllItems();
    await request
      .post('/api/orders')
      .send({ items: [{ inventoryId: items[0].id, quantity: 1 }] });
    
    const response = await request.get('/api/orders?status=pending');
    
    assert.strictEqual(response.status, 200);
    assert.ok(Array.isArray(response.body.data));
  });

  it('POST /api/orders should create a new order', async () => {
    const items = inventoryService.getAllItems();
    const item = items[0];
    const initialQuantity = item.quantity;
    
    const orderData = {
      items: [{ inventoryId: item.id, quantity: 2 }]
    };
    
    const response = await request
      .post('/api/orders')
      .send(orderData);
    
    assert.strictEqual(response.status, 201);
    assert.strictEqual(response.body.success, true);
    assert.ok(response.body.data.id);
    assert.strictEqual(response.body.data.status, 'pending');
    assert.ok(response.body.data.items.length > 0);
  });

  it('POST /api/orders should fail with invalid items', async () => {
    const response = await request
      .post('/api/orders')
      .send({ items: [] });
    
    assert.strictEqual(response.status, 400);
    assert.strictEqual(response.body.success, false);
  });

  it('POST /api/orders should fail with insufficient inventory', async () => {
    const items = inventoryService.getAllItems();
    const item = items[0];
    
    // Try to order more than available
    const response = await request
      .post('/api/orders')
      .send({ items: [{ inventoryId: item.id, quantity: 9999 }] });
    
    assert.strictEqual(response.status, 400);
  });

  it('PATCH /api/orders/:id/status should update order status', async () => {
    const items = inventoryService.getAllItems();
    
    // Create an order
    const createResponse = await request
      .post('/api/orders')
      .send({ items: [{ inventoryId: items[0].id, quantity: 1 }] });
    
    const orderId = createResponse.body.data.id;
    
    // Update status
    const response = await request
      .patch(`/api/orders/${orderId}/status`)
      .send({ status: 'confirmed' });
    
    assert.strictEqual(response.status, 200);
    assert.strictEqual(response.body.success, true);
    assert.strictEqual(response.body.data.status, 'confirmed');
  });

  it('PATCH /api/orders/:id/status should fail with invalid status', async () => {
    const items = inventoryService.getAllItems();
    
    const createResponse = await request
      .post('/api/orders')
      .send({ items: [{ inventoryId: items[0].id, quantity: 1 }] });
    
    const orderId = createResponse.body.data.id;
    
    const response = await request
      .patch(`/api/orders/${orderId}/status`)
      .send({ status: 'invalid_status' });
    
    assert.strictEqual(response.status, 400);
  });

  it('DELETE /api/orders/:id should delete an order', async () => {
    const items = inventoryService.getAllItems();
    
    const createResponse = await request
      .post('/api/orders')
      .send({ items: [{ inventoryId: items[0].id, quantity: 1 }] });
    
    const orderId = createResponse.body.data.id;
    
    const deleteResponse = await request.delete(`/api/orders/${orderId}`);
    
    assert.strictEqual(deleteResponse.status, 200);
    assert.strictEqual(deleteResponse.body.success, true);
    
    // Verify deletion
    const getResponse = await request.get(`/api/orders/${orderId}`);
    assert.strictEqual(getResponse.status, 404);
  });
});

describe('Metrics Endpoint', () => {
  beforeEach(() => {
    metricsService.resetMetrics();
    inventoryService.clearInventory();
    inventoryService.initializeSampleData();
  });

  it('GET /metrics should return all metrics', async () => {
    const response = await request.get('/metrics');
    
    assert.strictEqual(response.status, 200);
    assert.strictEqual(response.body.success, true);
    assert.ok(response.body.data.requests);
    assert.ok(response.body.data.inventory);
    assert.ok(response.body.data.orders);
  });

  it('GET /metrics should track requests', async () => {
    // Make some requests
    await request.get('/health');
    await request.get('/api/inventory');
    
    const response = await request.get('/metrics');
    
    assert.ok(response.body.data.requests.total >= 2);
  });

  it('GET /metrics/prometheus should return Prometheus format', async () => {
    const response = await request.get('/metrics/prometheus');
    
    assert.strictEqual(response.status, 200);
    assert.ok(response.text.includes('http_requests_total'));
    assert.ok(response.text.includes('inventory_items_total'));
    assert.ok(response.text.includes('orders_total'));
  });
});

describe('Error Handling', () => {
  it('should return 404 for unknown endpoints', async () => {
    const response = await request.get('/unknown-endpoint');
    
    assert.strictEqual(response.status, 404);
    assert.strictEqual(response.body.success, false);
  });
});
