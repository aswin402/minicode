import {
  Order,
  CreateOrderDTO,
  OrderStatus,
  OrderItem,
  ApiResponse
} from '../types';
import * as inventoryService from './inventory.service';

// In-memory storage
const orders: Map<string, Order> = new Map();

// Helper to generate UUID
function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

// Get all orders
export function getAllOrders(): Order[] {
  return Array.from(orders.values());
}

// Get order by ID
export function getOrderById(id: string): Order | undefined {
  return orders.get(id);
}

// Create new order
export function createOrder(data: CreateOrderDTO): Order | null {
  // Validate all items exist and have sufficient quantity
  const orderItems: OrderItem[] = [];
  let totalAmount = 0;
  
  for (const item of data.items) {
    const inventoryItem = inventoryService.getItemById(item.inventoryId);
    if (!inventoryItem) {
      return null; // Item not found
    }
    
    // Check availability
    if (!inventoryService.checkAvailability(item.inventoryId, item.quantity)) {
      return null; // Insufficient quantity
    }
    
    // Calculate item total
    const itemTotal = inventoryItem.price * item.quantity;
    totalAmount += itemTotal;
    
    orderItems.push({
      inventoryId: item.inventoryId,
      quantity: item.quantity,
      price: inventoryItem.price
    });
    
    // Reduce inventory
    inventoryService.reduceQuantity(item.inventoryId, item.quantity);
  }
  
  const now = new Date().toISOString();
  const order: Order = {
    id: generateId(),
    items: orderItems,
    totalAmount: Math.round(totalAmount * 100) / 100,
    status: 'pending',
    createdAt: now,
    updatedAt: now
  };
  
  orders.set(order.id, order);
  return order;
}

// Update order status
export function updateOrderStatus(id: string, status: OrderStatus): Order | null {
  const order = orders.get(id);
  if (!order) {
    return null;
  }
  
  const updatedOrder: Order = {
    ...order,
    status,
    updatedAt: new Date().toISOString()
  };
  
  orders.set(id, updatedOrder);
  return updatedOrder;
}

// Get orders by status
export function getOrdersByStatus(status: OrderStatus): Order[] {
  return Array.from(orders.values()).filter(order => order.status === status);
}

// Get order statistics
export function getOrderStats(): Record<OrderStatus, number> {
  const allOrders = Array.from(orders.values());
  const stats: Record<OrderStatus, number> = {
    pending: 0,
    confirmed: 0,
    shipped: 0,
    delivered: 0,
    cancelled: 0
  };
  
  for (const order of allOrders) {
    stats[order.status]++;
  }
  
  return stats;
}

// Delete order
export function deleteOrder(id: string): boolean {
  return orders.delete(id);
}

// Clear all orders (for testing)
export function clearOrders(): void {
  orders.clear();
}

// Get total orders count
export function getTotalOrdersCount(): number {
  return orders.size;
}
