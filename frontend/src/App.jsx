import React, { useState } from 'react';
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

export default function App() {
  const [currentRoute, setCurrentRoute] = useState('home');
  const [selectedTaskId, setSelectedTaskId] = useState('TSK-019');

  const handleSelectTask = (taskId) => {
    setSelectedTaskId(taskId);
    setCurrentRoute('work');
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  const navigateTo = (route) => {
    setCurrentRoute(route);
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  return (
    <div style={{ minHeight: '100vh', display: 'flex', flexDirection: 'column' }}>
      {/* Top Header matching PDF Mockup */}
      <Header
        onNavigate={navigateTo}
        onOpenSignIn={() => navigateTo('signin')}
        onOpenContact={() => navigateTo('contact')}
      />

      {/* Main Pages matching PDF Pages 1 to 8 */}
      <main style={{ flex: 1 }}>
        {currentRoute === 'home' && (
          <HomePage onNavigate={navigateTo} />
        )}

        {currentRoute === 'dashboard' && (
          <DashboardPage onSelectTask={handleSelectTask} />
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
          <NewTaskPage onCreated={() => navigateTo('tasks')} />
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

      {/* Bottom Floating Navigation Dock matching PDF Mockup */}
      <Navbar
        currentRoute={currentRoute}
        onNavigate={navigateTo}
      />
    </div>
  );
}
