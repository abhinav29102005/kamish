import React from "react";

export default function Navbar({ currentRoute, onNavigate }) {
  const navItems = [
    { id: "home", label: "Home" },
    { id: "dashboard", label: "Dashboard" },
    { id: "topics", label: "Topics Catalog" },
    { id: "console", label: "Produce/Fetch" },
    { id: "authority", label: "Authority & Policy" },
    { id: "audit", label: "Audit & Checkers" },
  ];

  return (
    <nav className="mockup-dock" aria-label="Bottom Navigation Pill">
      {navItems.map((item) => (
        <button
          key={item.id}
          id={`nav-${item.id}`}
          className={`dock-pill ${currentRoute === item.id ? "active" : ""}`}
          onClick={() => onNavigate(item.id)}
        >
          {item.label}
        </button>
      ))}
    </nav>
  );
}
