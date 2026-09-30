import { Router, Request, Response } from 'express';
import { ApiResponse, Order, CreateOrderDTO, OrderStatus, ValidationResult } from '../types';
import * as ordersService from '../services/orders.service';
import * as inventoryService from '../services/inventory.service';
import { validateArray, createValidationResult } from '../utils/validation';

const router = Router();

// Valid order statuses
const ORDER_STATUSES: readonly OrderStatus[] = ['pending', 'confirmed', 'shipped', 'delivered', 'cancelled'];

// Validate create order request
function validateCreateOrder(body: unknown): ValidationResult {
  const data = body as { items?: unknown };
  const errors: (string | null)[] = [];
  
  const items = data.items;
  if (!items || !Array.isArray(items)) {
    errors.push('items: must be an array with at least 1 item(s)');
  } else if (items.length === 0) {
    errors.push('items: must have at least 1 item(s)');
  } else {
    for (let i = 0; i < items.length; i++) {
      const item = items[i] as { inventoryId?: unknown; quantity?: unknown };
      
      if (!item.inventoryId || typeof item.inventoryId !== 'string') {
        errors.push(`items[${i}].inventoryId: must be a string`);
      } else if (!inventoryService.itemExists(item.inventoryId)) {
        errors.push(`items[${i}].inventoryId: item does not exist`);
      }
      
      if (item.quantity === undefined || typeof item.quantity !== 'number' || item.quantity <= 0) {
        errors.push(`items[${i}].quantity: must be a positive number`);
      } else if (item.inventoryId && typeof item.inventoryId === 'string' && !inventoryService.checkAvailability(item.inventoryId, item.quantity)) {
        errors.push(`items[${i}]: insufficient quantity available`);
      }
    }
  }
  
  return createValidationResult(errors);
}

// Validate order status
function validateOrderStatus(status: unknown): status is OrderStatus {
  return ORDER_STATUSES.includes(status as OrderStatus);
}

// GET /api/orders - Get all orders
router.get('/', (req: Request, res: Response) => {
  const { status } = req.query;
  
  let orders: Order[];
  
  if (status && validateOrderStatus(status)) {
    orders = ordersService.getOrdersByStatus(status);
  } else {
    orders = ordersService.getAllOrders();
  }
  
  res.json({
    success: true,
    data: orders,
    message: `Retrieved ${orders.length} order(s)`
  });
});

// GET /api/orders/:id - Get order by ID
router.get('/:id', (req: Request, res: Response) => {
  const { id } = req.params;
  const order = ordersService.getOrderById(id);
  
  if (!order) {
    res.status(404).json({
      success: false,
      error: 'Order not found'
    });
    return;
  }
  
  res.json({
    success: true,
    data: order
  });
});

// POST /api/orders - Create new order
router.post('/', (req: Request, res: Response) => {
  const validation = validateCreateOrder(req.body);
  
  if (!validation.valid) {
    res.status(400).json({
      success: false,
      error: 'Validation failed',
      data: validation.errors
    });
    return;
  }
  
  const order = ordersService.createOrder(req.body as CreateOrderDTO);
  
  if (!order) {
    res.status(400).json({
      success: false,
      error: 'Failed to create order. Items may not exist or have insufficient quantity.'
    });
    return;
  }
  
  res.status(201).json({
    success: true,
    data: order,
    message: 'Order created successfully'
  });
});

// PATCH /api/orders/:id/status - Update order status
router.patch('/:id/status', (req: Request, res: Response) => {
  const { id } = req.params;
  const { status } = req.body;
  
  if (!validateOrderStatus(status)) {
    res.status(400).json({
      success: false,
      error: `Invalid status. Must be one of: ${ORDER_STATUSES.join(', ')}`
    });
    return;
  }
  
  const order = ordersService.updateOrderStatus(id, status);
  
  if (!order) {
    res.status(404).json({
      success: false,
      error: 'Order not found'
    });
    return;
  }
  
  res.json({
    success: true,
    data: order,
    message: `Order status updated to ${status}`
  });
});

// DELETE /api/orders/:id - Delete order
router.delete('/:id', (req: Request, res: Response) => {
  const { id } = req.params;
  const deleted = ordersService.deleteOrder(id);
  
  if (!deleted) {
    res.status(404).json({
      success: false,
      error: 'Order not found'
    });
    return;
  }
  
  res.json({
    success: true,
    message: 'Order deleted successfully'
  });
});

export default router;
