import React, { useState, useEffect, useCallback } from 'react';
import { KamishProvider } from './context/KamishContext';
import Header from './components/Header';
import Navbar from './components/Navbar';
import HomePage from './pages/HomePage';
import DashboardPage from './pages/DashboardPage';
import TasksPage from './pages/TasksPage';
import WorkTaskPage from './pages/WorkTaskPage';
import NewTaskPage from './pages/NewTaskPage';
import ContactPage from './pages/ContactPage';
import SignInPage from './pages/SignInPage';
import NotFoundPage from './pages/NotFoundPage';

function parseHashRoute() {
  let hash = window.location.hash || '';
  if (hash.startsWith('#/')) {
    hash = hash.slice(2);
  } else if (hash.startsWith('#')) {
    hash = hash.slice(1);
  }
  
  if (!hash) {
    const path = window.location.pathname.replace(/^\//, '');
    if (path && ['home', 'dashboard', 'tasks', 'work', 'new', 'contact', 'signin', '404'].includes(path.split('/')[0])) {
      hash = path;
    }
  }

  const parts = hash.split('/').filter(Boolean);
  const route = parts[0] || 'home';
  const param = parts[1] || null;

  return { route, param };
}

function AppContent() {
  const [currentRoute, setCurrentRoute] = useState('home');
  const [selectedTaskId, setSelectedTaskId] = useState('TSK-001');

  // Sync route from URL hash
  const syncFromHash = useCallback(() => {
    const { route, param } = parseHashRoute();
    const validRoutes = ['home', 'dashboard', 'tasks', 'work', 'new', 'contact', 'signin', '404'];
    
    if (validRoutes.includes(route)) {
      setCurrentRoute(route);
      if (route === 'work' && param) {
        setSelectedTaskId(param);
      }
    } else {
      setCurrentRoute('404');
    }
  }, []);

  useEffect(() => {
    syncFromHash();
    window.addEventListener('hashchange', syncFromHash);
    window.addEventListener('popstate', syncFromHash);

    return () => {
      window.removeEventListener('hashchange', syncFromHash);
      window.removeEventListener('popstate', syncFromHash);
    };
  }, [syncFromHash]);

  const navigateTo = (route, param = null) => {
    let targetHash = '#/' + route;
    if (route === 'work') {
      const taskId = param || selectedTaskId || 'TSK-001';
      setSelectedTaskId(taskId);
      targetHash = '#/work/' + taskId;
    } else if (param) {
      targetHash += '/' + param;
    }

    if (window.location.hash !== targetHash) {
      window.location.hash = targetHash;
    } else {
      syncFromHash();
    }
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  const handleSelectTask = (taskId) => {
    setSelectedTaskId(taskId);
    navigateTo('work', taskId);
  };

  return (
    <div style={{ minHeight: '100vh', display: 'flex', flexDirection: 'column' }}>
      {/* Top Header with Brand, Live Status, 3D Story Link, and Contact */}
      <Header
        onNavigate={navigateTo}
        onOpenContact={() => navigateTo('contact')}
      />

      {/* Main Routed Content */}
      <main style={{ flex: 1 }}>
        {currentRoute === 'home' && (
          <HomePage onNavigate={navigateTo} />
        )}

        {currentRoute === 'dashboard' && (
          <DashboardPage
            onSelectTask={handleSelectTask}
            onNavigate={navigateTo}
          />
        )}

        {currentRoute === 'tasks' && (
          <TasksPage
            onSelectTask={handleSelectTask}
            onNewTask={() => navigateTo('new')}
          />
        )}

        {currentRoute === 'work' && (
          <WorkTaskPage
            taskId={selectedTaskId}
            onBack={() => navigateTo('tasks')}
          />
        )}

        {currentRoute === 'new' && (
          <NewTaskPage onCreated={() => navigateTo('dashboard')} />
        )}

        {currentRoute === 'contact' && (
          <ContactPage />
        )}

        {currentRoute === 'signin' && (
          <SignInPage onSignedIn={() => navigateTo('dashboard')} />
        )}

        {currentRoute === '404' && (
          <NotFoundPage onBack={() => navigateTo('home')} />
        )}
      </main>

      {/* Bottom Floating Navigation Dock */}
      <Navbar
        currentRoute={currentRoute}
        onNavigate={navigateTo}
      />
    </div>
  );
}

export default function App() {
  return (
    <KamishProvider>
      <AppContent />
    </KamishProvider>
  );
}
