import { Router, Request, Response } from 'express';
import { HealthStatus } from '../types';

const router = Router();

// Store server start time
const serverStartTime = Date.now();

// GET /health - Health check endpoint
router.get('/health', (_req: Request, res: Response) => {
  const uptime = Math.floor((Date.now() - serverStartTime) / 1000);
  const memoryUsage = process.memoryUsage();
  
  const healthStatus: HealthStatus = {
    status: 'healthy',
    uptime,
    memory: {
      used: Math.round(memoryUsage.heapUsed / 1024 / 1024 * 100) / 100,
      total: Math.round(memoryUsage.heapTotal / 1024 / 1024 * 100) / 100,
      unit: 'MB'
    },
    timestamp: new Date().toISOString()
  };
  
  res.json({
    success: true,
    data: healthStatus
  });
});

export default router;
