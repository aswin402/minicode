import { Metrics } from '../types';
import * as inventoryService from './inventory.service';
import * as ordersService from './orders.service';

// Request counter metrics
let totalRequests = 0;
let successRequests = 0;
let errorRequests = 0;

// Track requests
export function incrementTotalRequests(): void {
  totalRequests++;
}

export function incrementSuccessRequests(): void {
  successRequests++;
}

export function incrementErrorRequests(): void {
  errorRequests++;
}

// Get all metrics
export function getMetrics(): Metrics {
  const inventoryStats = inventoryService.getInventoryStats();
  const orderStats = ordersService.getOrderStats();
  
  return {
    requests: {
      total: totalRequests,
      success: successRequests,
      errors: errorRequests
    },
    inventory: {
      totalItems: inventoryStats.totalItems,
      totalValue: inventoryStats.totalValue
    },
    orders: {
      total: ordersService.getTotalOrdersCount(),
      ...orderStats
    }
  };
}

// Reset metrics (for testing)
export function resetMetrics(): void {
  totalRequests = 0;
  successRequests = 0;
  errorRequests = 0;
}
