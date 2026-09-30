import { Router, Request, Response } from 'express';
import { Metrics } from '../types';
import * as metricsService from '../services/metrics.service';

const router = Router();

// GET /metrics - Get all metrics
router.get('/', (_req: Request, res: Response) => {
  const metrics = metricsService.getMetrics();
  
  res.json({
    success: true,
    data: metrics
  });
});

// GET /metrics/prometheus - Get metrics in Prometheus format
router.get('/prometheus', (_req: Request, res: Response) => {
  const metrics = metricsService.getMetrics();
  
  // Format as Prometheus-compatible text
  const prometheusOutput = [
    `# HELP http_requests_total Total HTTP requests`,
    `# TYPE http_requests_total counter`,
    `http_requests_total{type="total"} ${metrics.requests.total}`,
    `http_requests_total{type="success"} ${metrics.requests.success}`,
    `http_requests_total{type="errors"} ${metrics.requests.errors}`,
    ``,
    `# HELP inventory_items_total Total inventory items`,
    `# TYPE inventory_items_total gauge`,
    `inventory_items_total ${metrics.inventory.totalItems}`,
    ``,
    `# HELP inventory_total_value Total inventory value`,
    `# TYPE inventory_total_value gauge`,
    `inventory_total_value ${metrics.inventory.totalValue}`,
    ``,
    `# HELP orders_total Total orders`,
    `# TYPE orders_total counter`,
    `orders_total ${metrics.orders.total}`,
    `orders_total{status="pending"} ${metrics.orders.pending}`,
    `orders_total{status="confirmed"} ${metrics.orders.confirmed}`,
    `orders_total{status="shipped"} ${metrics.orders.shipped}`,
    `orders_total{status="delivered"} ${metrics.orders.delivered}`,
    `orders_total{status="cancelled"} ${metrics.orders.cancelled}`
  ].join('\n');
  
  res.set('Content-Type', 'text/plain');
  res.send(prometheusOutput);
});

export default router;
