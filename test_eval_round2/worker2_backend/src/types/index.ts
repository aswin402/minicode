// Inventory Types
export interface InventoryItem {
  id: string;
  name: string;
  description: string;
  quantity: number;
  price: number;
  category: string;
  createdAt: string;
  updatedAt: string;
}

export interface CreateInventoryItemDTO {
  name: string;
  description: string;
  quantity: number;
  price: number;
  category: string;
}

export interface UpdateInventoryItemDTO {
  name?: string;
  description?: string;
  quantity?: number;
  price?: number;
  category?: string;
}

// Order Types
export interface OrderItem {
  inventoryId: string;
  quantity: number;
  price: number;
}

export interface Order {
  id: string;
  items: OrderItem[];
  totalAmount: number;
  status: OrderStatus;
  createdAt: string;
  updatedAt: string;
}

export type OrderStatus = 'pending' | 'confirmed' | 'shipped' | 'delivered' | 'cancelled';

export interface CreateOrderDTO {
  items: {
    inventoryId: string;
    quantity: number;
  }[];
}

// Health Check Types
export interface HealthStatus {
  status: 'healthy' | 'unhealthy';
  uptime: number;
  memory: {
    used: number;
    total: number;
    unit: string;
  };
  timestamp: string;
}

// Metrics Types
export interface Metrics {
  requests: {
    total: number;
    success: number;
    errors: number;
  };
  inventory: {
    totalItems: number;
    totalValue: number;
  };
  orders: {
    total: number;
    pending: number;
    confirmed: number;
    shipped: number;
    delivered: number;
    cancelled: number;
  };
}

// API Response Types
export interface ApiResponse<T = unknown> {
  success: boolean;
  data?: T;
  error?: string;
  message?: string;
}

// Validation Types
export interface ValidationError {
  field: string;
  message: string;
}

export interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
}
