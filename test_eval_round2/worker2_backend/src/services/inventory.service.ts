import {
  InventoryItem,
  CreateInventoryItemDTO,
  UpdateInventoryItemDTO,
} from '../types';

// In-memory storage
const inventory: Map<string, InventoryItem> = new Map();

// Helper to generate UUID
function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

// Get all inventory items
export function getAllItems(): InventoryItem[] {
  return Array.from(inventory.values());
}

// Get item by ID
export function getItemById(id: string): InventoryItem | undefined {
  return inventory.get(id);
}

// Create new inventory item
export function createItem(data: CreateInventoryItemDTO): InventoryItem {
  const now = new Date().toISOString();
  const item: InventoryItem = {
    id: generateId(),
    name: data.name,
    description: data.description,
    quantity: data.quantity,
    price: data.price,
    category: data.category,
    createdAt: now,
    updatedAt: now
  };
  inventory.set(item.id, item);
  return item;
}

// Update inventory item
export function updateItem(id: string, data: UpdateInventoryItemDTO): InventoryItem | null {
  const existingItem = inventory.get(id);
  if (!existingItem) {
    return null;
  }
  
  const updatedItem: InventoryItem = {
    ...existingItem,
    ...data,
    updatedAt: new Date().toISOString()
  };
  
  inventory.set(id, updatedItem);
  return updatedItem;
}

// Delete inventory item
export function deleteItem(id: string): boolean {
  return inventory.delete(id);
}

// Check if item exists
export function itemExists(id: string): boolean {
  return inventory.has(id);
}

// Check if sufficient quantity is available
export function checkAvailability(id: string, requestedQuantity: number): boolean {
  const item = inventory.get(id);
  if (!item) return false;
  return item.quantity >= requestedQuantity;
}

// Reduce inventory quantity
export function reduceQuantity(id: string, quantity: number): boolean {
  const item = inventory.get(id);
  if (!item || item.quantity < quantity) return false;
  
  item.quantity -= quantity;
  item.updatedAt = new Date().toISOString();
  inventory.set(id, item);
  return true;
}

// Get inventory statistics
export function getInventoryStats(): { totalItems: number; totalValue: number } {
  const items = Array.from(inventory.values());
  return {
    totalItems: items.length,
    totalValue: items.reduce((sum, item) => sum + (item.price * item.quantity), 0)
  };
}

// Clear all inventory (for testing)
export function clearInventory(): void {
  inventory.clear();
}

// Initialize with sample data
export function initializeSampleData(): void {
  const sampleItems: CreateInventoryItemDTO[] = [
    { name: 'Laptop', description: 'High-performance laptop', quantity: 10, price: 999.99, category: 'electronics' },
    { name: 'Mouse', description: 'Wireless mouse', quantity: 50, price: 29.99, category: 'electronics' },
    { name: 'Keyboard', description: 'Mechanical keyboard', quantity: 30, price: 89.99, category: 'electronics' },
    { name: 'Monitor', description: '27-inch 4K monitor', quantity: 15, price: 399.99, category: 'electronics' },
    { name: 'Headphones', description: 'Noise-canceling headphones', quantity: 25, price: 199.99, category: 'electronics' }
  ];
  
  for (const item of sampleItems) {
    createItem(item);
  }
}
