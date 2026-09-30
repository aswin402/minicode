import { Router, Request, Response } from 'express';
import { ApiResponse, InventoryItem, CreateInventoryItemDTO, UpdateInventoryItemDTO, ValidationResult } from '../types';
import * as inventoryService from '../services/inventory.service';
import { validateString, validateNumber, validatePositiveNumber, createValidationResult } from '../utils/validation';

const router = Router();

// Validate create item request
function validateCreateItem(body: unknown): ValidationResult {
  const data = body as Record<string, unknown>;
  const errors: (string | null)[] = [];
  
  errors.push(validateString(data.name, 'name'));
  errors.push(validateString(data.description, 'description'));
  
  const qtyError = validatePositiveNumber(data.quantity, 'quantity');
  errors.push(qtyError !== null ? null : validatePositiveNumber(data.quantity, 'quantity'));
  
  const priceError = validateNumber(data.price, 'price', 0);
  errors.push(priceError !== null ? null : validateNumber(data.price, 'price', 0));
  
  errors.push(validateString(data.category, 'category'));
  
  return createValidationResult(errors);
}

// Validate update item request
function validateUpdateItem(body: unknown): ValidationResult {
  const data = body as Record<string, unknown>;
  const errors: (string | null)[] = [];
  
  if (data.name !== undefined) errors.push(validateString(data.name, 'name'));
  if (data.description !== undefined) errors.push(validateString(data.description, 'description'));
  if (data.quantity !== undefined) errors.push(validatePositiveNumber(data.quantity, 'quantity'));
  if (data.price !== undefined) errors.push(validateNumber(data.price, 'price', 0));
  if (data.category !== undefined) errors.push(validateString(data.category, 'category'));
  
  return createValidationResult(errors);
}

// GET /api/inventory - Get all inventory items
router.get('/', (_req: Request, res: Response) => {
  const items = inventoryService.getAllItems();
  
  const response: ApiResponse<InventoryItem[]> = {
    success: true,
    data: items,
    message: `Retrieved ${items.length} inventory item(s)`
  };
  
  res.json(response);
});

// GET /api/inventory/:id - Get inventory item by ID
router.get('/:id', (req: Request, res: Response) => {
  const { id } = req.params;
  const item = inventoryService.getItemById(id);
  
  if (!item) {
    res.status(404).json({
      success: false,
      error: 'Inventory item not found'
    });
    return;
  }
  
  res.json({
    success: true,
    data: item
  });
});

// POST /api/inventory - Create new inventory item
router.post('/', (req: Request, res: Response) => {
  const validation = validateCreateItem(req.body);
  
  if (!validation.valid) {
    res.status(400).json({
      success: false,
      error: 'Validation failed',
      data: validation.errors
    });
    return;
  }
  
  const item = inventoryService.createItem(req.body as CreateInventoryItemDTO);
  
  res.status(201).json({
    success: true,
    data: item,
    message: 'Inventory item created successfully'
  });
});

// PUT /api/inventory/:id - Update inventory item
router.put('/:id', (req: Request, res: Response) => {
  const { id } = req.params;
  const validation = validateUpdateItem(req.body);
  
  if (!validation.valid) {
    res.status(400).json({
      success: false,
      error: 'Validation failed',
      data: validation.errors
    });
    return;
  }
  
  const item = inventoryService.updateItem(id, req.body as UpdateInventoryItemDTO);
  
  if (!item) {
    res.status(404).json({
      success: false,
      error: 'Inventory item not found'
    });
    return;
  }
  
  res.json({
    success: true,
    data: item,
    message: 'Inventory item updated successfully'
  });
});

// DELETE /api/inventory/:id - Delete inventory item
router.delete('/:id', (req: Request, res: Response) => {
  const { id } = req.params;
  const deleted = inventoryService.deleteItem(id);
  
  if (!deleted) {
    res.status(404).json({
      success: false,
      error: 'Inventory item not found'
    });
    return;
  }
  
  res.json({
    success: true,
    message: 'Inventory item deleted successfully'
  });
});

export default router;
