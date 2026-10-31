import { useContext } from 'react';
import { DemoTasksContext } from './demoTasksContext.js';

export function useDemoTasks() {
  const context = useContext(DemoTasksContext);
  if (!context) throw new Error('Demo task provider is required');
  return context;
}
