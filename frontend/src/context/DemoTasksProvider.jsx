import { useState } from 'react';
import { DemoTasksContext } from './demoTasksContext.js';
import { loadDemoTasks, saveDemoTask } from '../utils/demoTasks.js';

export default function DemoTasksProvider({ children }) {
  const [initial] = useState(() => loadDemoTasks({ getItem: key => window.localStorage.getItem(key) }));
  const [tasks, setTasks] = useState(initial.tasks);

  const createDemoTask = (fields) => {
    if (initial.error) throw new Error(initial.error);
    const latest = loadDemoTasks({ getItem: key => window.localStorage.getItem(key) });
    if (latest.error) throw new Error(latest.error);
    const task = {
      ...fields, id: `DEMO-${crypto.randomUUID()}`,
      title: fields.title.trim(), owner: fields.owner.trim(),
      rule: 'Locally saved demo task; no broker dispatch or verification',
    };
    const next = saveDemoTask(window.localStorage, latest.tasks, task);
    setTasks(next);
    return task;
  };
  return <DemoTasksContext.Provider value={{ tasks, createDemoTask, storageError: initial.error }}>
    {children}
  </DemoTasksContext.Provider>;
}
