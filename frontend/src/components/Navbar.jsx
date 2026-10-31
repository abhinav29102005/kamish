import React from 'react';

export default function Navbar({ currentRoute, onNavigate }) {
  const navItems = [
    { id: 'home', label: 'Home' },
    { id: 'dashboard', label: 'Dashboard' },
    { id: 'tasks', label: 'Tasks' },
    { id: 'new', label: 'New' },
    { id: 'contact', label: 'Contact' },
  ];

  return (
    <nav className="mockup-dock" aria-label="Bottom Navigation Pill">
      {navItems.map((item) => (
        <button
          key={item.id}
          className={`dock-pill ${currentRoute === item.id ? 'active' : ''}`}
          onClick={() => onNavigate(item.id)}
        >
          {item.label}
        </button>
      ))}
    </nav>
  );
}
