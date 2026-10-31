import { lazy, Suspense, useState, useEffect } from 'react';
import { KamishProvider } from './context/KamishContext';
import DemoTasksProvider from './context/DemoTasksProvider.jsx';
import Header from './components/Header';
import Navbar from './components/Navbar';
const HomePage = lazy(() => import('./pages/HomePage.jsx'));
const DashboardPage = lazy(() => import('./pages/DashboardPage.jsx'));
const TasksPage = lazy(() => import('./pages/TasksPage.jsx'));
const WorkTaskPage = lazy(() => import('./pages/WorkTaskPage.jsx'));
const NewTaskPage = lazy(() => import('./pages/NewTaskPage.jsx'));
const ContactPage = lazy(() => import('./pages/ContactPage.jsx'));
const SignInPage = lazy(() => import('./pages/SignInPage.jsx'));
const NotFoundPage = lazy(() => import('./pages/NotFoundPage.jsx'));

function parseHashRoute() {
  let hash = window.location.hash || '';
  if (hash.startsWith('#/')) {
    hash = hash.slice(2);
  } else if (hash.startsWith('#')) {
    hash = hash.slice(1);
  }
  
  if (!hash) {
    const path = window.location.pathname.replace(/^\//, '');
    if (path) {
      hash = path;
    }
  }

  const parts = hash.split('/').filter(Boolean);
  let route = parts[0] || 'home';
  let param = parts[1] || null;
  try { param = param === null ? null : decodeURIComponent(param); }
  catch { route = '404'; param = null; }
  if (!['home', 'dashboard', 'tasks', 'work', 'new', 'contact', 'signin', '404'].includes(route)
    || parts.length > (route === 'work' ? 2 : 1) || (route === 'work' && !param)) route = '404';

  return { route, param };
}

function AppContent() {
  const [location, setLocation] = useState(parseHashRoute);
  const currentRoute = location.route;
  const selectedTaskId = location.param;

  // Sync route from URL hash
  useEffect(() => {
    const syncFromHash = () => setLocation(parseHashRoute());
    window.addEventListener('hashchange', syncFromHash);
    window.addEventListener('popstate', syncFromHash);

    return () => {
      window.removeEventListener('hashchange', syncFromHash);
      window.removeEventListener('popstate', syncFromHash);
    };
  }, []);

  const navigateTo = (route, param = null) => {
    let targetHash = '#/' + route;
    if (route === 'work') {
      const taskId = param || selectedTaskId;
      targetHash = taskId ? '#/work/' + encodeURIComponent(taskId) : '#/404';
    } else if (param) {
      targetHash += '/' + param;
    }

    if (window.location.hash !== targetHash) {
      window.location.hash = targetHash;
    } else {
      setLocation(parseHashRoute());
    }
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  const handleSelectTask = (taskId) => {
    navigateTo('work', taskId);
  };

  return (
    <div style={{ minHeight: '100vh', display: 'flex', flexDirection: 'column' }}>
      {/* Top Header with Brand, Live Status, 3D Story Link, and Contact */}
      <Header
        onNavigate={navigateTo}
        onOpenContact={() => navigateTo('contact')}
      />
      <div role="status" style={{ padding: '0.75rem 2rem', textAlign: 'center', color: '#d8b4fe' }}>
        DEMO / SIMULATED — no live broker connection, user authentication, or message delivery.
      </div>

      {/* Main Routed Content */}
      <main style={{ flex: 1 }}>
        <Suspense fallback={<p role="status">Loading demo page…</p>}>
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
          <NewTaskPage onCreated={task => navigateTo('work', task.id)} />
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
        </Suspense>
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
      <DemoTasksProvider><AppContent /></DemoTasksProvider>
    </KamishProvider>
  );
}
