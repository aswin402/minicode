import { ValidationResult, ValidationError } from '../types';

export function validateString(value: unknown, fieldName: string): string | null {
  if (typeof value !== 'string' || value.trim() === '') {
    return `${fieldName} must be a non-empty string`;
  }
  return null;
}

export function validateNumber(value: unknown, fieldName: string, min?: number, max?: number): string | null {
  const num = Number(value);
  if (isNaN(num)) {
    return `${fieldName} must be a valid number`;
  }
  if (min !== undefined && num < min) {
    return `${fieldName} must be at least ${min}`;
  }
  if (max !== undefined && num > max) {
    return `${fieldName} must be at most ${max}`;
  }
  return null;
}

export function validatePositiveNumber(value: unknown, fieldName: string): string | null {
  const num = Number(value);
  if (isNaN(num) || num < 0) {
    return `${fieldName} must be a non-negative number`;
  }
  return null;
}

export function validateRequired(value: unknown, fieldName: string): string | null {
  if (value === undefined || value === null) {
    return `${fieldName} is required`;
  }
  return null;
}

export function validateEmail(value: unknown): string | null {
  if (typeof value !== 'string') {
    return 'Email must be a string';
  }
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  if (!emailRegex.test(value)) {
    return 'Invalid email format';
  }
  return null;
}

export function validateEnum<T extends string>(
  value: unknown,
  fieldName: string,
  allowedValues: readonly T[]
): string | null {
  if (typeof value !== 'string' || !allowedValues.includes(value as T)) {
    return `${fieldName} must be one of: ${allowedValues.join(', ')}`;
  }
  return null;
}

export function validateArray(
  value: unknown,
  fieldName: string,
  minLength?: number,
  maxLength?: number
): string | null {
  if (!Array.isArray(value)) {
    return `${fieldName} must be an array`;
  }
  if (minLength !== undefined && value.length < minLength) {
    return `${fieldName} must have at least ${minLength} item(s)`;
  }
  if (maxLength !== undefined && value.length > maxLength) {
    return `${fieldName} must have at most ${maxLength} item(s)`;
  }
  return null;
}

export function createValidationResult(errors: (string | null)[]): ValidationResult {
  const validationErrors: ValidationError[] = [];
  
  for (const error of errors) {
    if (error !== null) {
      const colonIndex = error.indexOf(':');
      const field = colonIndex > 0 ? error.substring(0, colonIndex) : error;
      const message = colonIndex > 0 ? error.substring(colonIndex + 1).trim() : error;
      validationErrors.push({ field, message });
    }
  }
  
  return {
    valid: validationErrors.length === 0,
    errors: validationErrors
  };
}
