import express, { Express, Request, Response, NextFunction } from 'express';
import healthRoutes from './routes/health.routes';
import inventoryRoutes from './routes/inventory.routes';
import ordersRoutes from './routes/orders.routes';
import metricsRoutes from './routes/metrics.routes';
import * as metricsService from './services/metrics.service';
import * as inventoryService from './services/inventory.service';
import { initializeSampleData } from './services/inventory.service';

// Create Express app
const app: Express = express();

// Middleware
app.use(express.json());

// Request logging and metrics
app.use((req: Request, res: Response, next: NextFunction) => {
  metricsService.incrementTotalRequests();
  
  const originalSend = res.send;
  res.send = function(body?: unknown): Response {
    if (res.statusCode >= 200 && res.statusCode < 400) {
      metricsService.incrementSuccessRequests();
    } else {
      metricsService.incrementErrorRequests();
    }
    return originalSend.call(this, body);
  };
  
  next();
});

// Routes
app.use('/', healthRoutes);
app.use('/api/inventory', inventoryRoutes);
app.use('/api/orders', ordersRoutes);
app.use('/metrics', metricsRoutes);

// 404 handler
app.use((_req: Request, res: Response) => {
  res.status(404).json({
    success: false,
    error: 'Endpoint not found'
  });
});

// Error handler
app.use((err: Error, _req: Request, res: Response, _next: NextFunction) => {
  console.error('Unhandled error:', err);
  res.status(500).json({
    success: false,
    error: 'Internal server error'
  });
});

// Start server
const PORT = process.env.PORT || 3000;

// Initialize sample data
inventoryService.initializeSampleData();

// Export for testing
export { app };

// Start server if not in test mode
if (process.env.NODE_ENV !== 'test') {
  app.listen(PORT, () => {
    process.stdout.write(`Server running on port ${PORT}\n`);
  });
}

export default app;
