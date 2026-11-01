import React from "react";
import { useKamish } from "../context/useKamish";

export default function Navbar({ currentRoute, onNavigate }) {
  const {
    isLiveMode,
    toggleMode,
    currentUser,
    logoutFromGateway,
  } = useKamish();

  const navItems = [
    { id: "home", label: "Home" },
    { id: "dashboard", label: "Dashboard" },
    { id: "topics", label: "Topics" },
    { id: "console", label: "Produce/Fetch" },
    { id: "authority", label: "Authority" },
    { id: "audit", label: "Audit" },
  ];

  return (
    <header
      role="banner"
      style={{
        position: "fixed",
        bottom: 0,
        left: 0,
        right: 0,
        zIndex: 100,
        height: "40px",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "0 1rem",
        background: "rgba(10, 5, 20, 0.96)",
        backdropFilter: "blur(16px)",
        WebkitBackdropFilter: "blur(16px)",
        borderTop: "1px solid rgba(168, 85, 247, 0.35)",
        boxShadow: "0 -4px 24px rgba(0, 0, 0, 0.75)",
        fontFamily: "var(--font-mono)",
        userSelect: "none",
      }}
    >
      {/* Centered, Contiguous Connected Console Strip */}
      <div
        style={{
          display: "inline-flex",
          alignItems: "stretch",
          height: "100%",
          borderLeft: "1px solid rgba(168, 85, 247, 0.3)",
          borderRight: "1px solid rgba(168, 85, 247, 0.3)",
          maxWidth: "100%",
          overflowX: "auto",
        }}
      >
        {/* 1. Mode Segment */}
        <span
          id="mode-badge"
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "6px",
            padding: "0 12px",
            fontSize: "0.72rem",
            fontWeight: 800,
            letterSpacing: "0.04em",
            color: isLiveMode ? "#4ade80" : "#facc15",
            background: isLiveMode ? "rgba(16, 185, 129, 0.12)" : "rgba(245, 158, 11, 0.12)",
            whiteSpace: "nowrap",
          }}
        >
          <span
            style={{
              width: "6px",
              height: "6px",
              background: isLiveMode ? "#4ade80" : "#facc15",
              borderRadius: "1px",
              display: "inline-block",
            }}
          />
          {isLiveMode ? "LIVE BROKER" : "DEMO / SIMULATION"}
        </span>

        <button
          id="mode-toggle-btn"
          onClick={toggleMode}
          style={{
            background: "transparent",
            border: "none",
            borderLeft: "1px solid rgba(168, 85, 247, 0.25)",
            borderRight: "1px solid rgba(168, 85, 247, 0.25)",
            color: "#94a3b8",
            fontFamily: "inherit",
            fontSize: "0.68rem",
            fontWeight: 700,
            padding: "0 10px",
            cursor: "pointer",
            letterSpacing: "0.05em",
            transition: "all 0.15s ease",
            whiteSpace: "nowrap",
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.color = "#ffffff";
            e.currentTarget.style.background = "rgba(168, 85, 247, 0.15)";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.color = "#94a3b8";
            e.currentTarget.style.background = "transparent";
          }}
          title="Toggle broker live connection vs local simulation"
        >
          {isLiveMode ? "SWITCH TO DEMO" : "SWITCH TO LIVE"}
        </button>

        {/* 2. Navigation Tabs (Contiguous Connected Strip) */}
        <nav
          aria-label="Main Navigation"
          style={{
            display: "inline-flex",
            alignItems: "stretch",
          }}
        >
          {navItems.map((item, idx) => {
            const isActive = currentRoute === item.id;
            return (
              <button
                key={item.id}
                id={`nav-${item.id}`}
                onClick={() => onNavigate(item.id)}
                style={{
                  background: isActive ? "#c084fc" : "transparent",
                  color: isActive ? "#0a0513" : "#cbd5e1",
                  border: "none",
                  borderRight: "1px solid rgba(168, 85, 247, 0.25)",
                  fontFamily: "var(--font-mono)",
                  fontSize: "0.76rem",
                  fontWeight: isActive ? 800 : 600,
                  letterSpacing: "0.06em",
                  textTransform: "uppercase",
                  padding: "0 16px",
                  height: "100%",
                  cursor: "pointer",
                  transition: "all 0.15s ease",
                  borderRadius: "0px",
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  whiteSpace: "nowrap",
                }}
                onMouseEnter={(e) => {
                  if (!isActive) {
                    e.currentTarget.style.background = "rgba(168, 85, 247, 0.15)";
                    e.currentTarget.style.color = "#ffffff";
                  }
                }}
                onMouseLeave={(e) => {
                  if (!isActive) {
                    e.currentTarget.style.background = "transparent";
                    e.currentTarget.style.color = "#cbd5e1";
                  }
                }}
              >
                {item.label}
              </button>
            );
          })}
        </nav>

        {/* 3. Auth & Session Segment */}
        {currentUser ? (
          <>
            <span
              id="user-badge"
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: "6px",
                padding: "0 10px",
                fontSize: "0.72rem",
                color: "#e9d5ff",
                background: "rgba(147, 51, 234, 0.15)",
                fontWeight: 700,
                borderRight: "1px solid rgba(168, 85, 247, 0.25)",
                whiteSpace: "nowrap",
              }}
            >
              <span>{currentUser.username}</span>
              <span
                style={{
                  background: "#581c87",
                  color: "#f3e8ff",
                  fontSize: "0.62rem",
                  padding: "1px 4px",
                  borderRadius: "2px",
                  textTransform: "uppercase",
                }}
              >
                {currentUser.role}
              </span>
            </span>

            <button
              id="logout-btn"
              onClick={logoutFromGateway}
              style={{
                background: "transparent",
                border: "none",
                color: "#fda4af",
                fontFamily: "inherit",
                fontSize: "0.68rem",
                fontWeight: 700,
                padding: "0 12px",
                cursor: "pointer",
                transition: "all 0.15s ease",
                whiteSpace: "nowrap",
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = "rgba(244, 63, 94, 0.2)";
                e.currentTarget.style.color = "#ffffff";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = "transparent";
                e.currentTarget.style.color = "#fda4af";
              }}
            >
              SIGN OUT
            </button>
          </>
        ) : (
          <button
            id="nav-signin-btn"
            onClick={() => onNavigate("signin")}
            style={{
              background: "rgba(168, 85, 247, 0.15)",
              border: "none",
              color: "#e9d5ff",
              fontFamily: "inherit",
              fontSize: "0.72rem",
              fontWeight: 700,
              padding: "0 14px",
              height: "100%",
              cursor: "pointer",
              letterSpacing: "0.06em",
              transition: "all 0.15s ease",
              whiteSpace: "nowrap",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = "#c084fc";
              e.currentTarget.style.color = "#0a0513";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = "rgba(168, 85, 247, 0.15)";
              e.currentTarget.style.color = "#e9d5ff";
            }}
          >
            SIGN IN
          </button>
        )}
      </div>
    </header>
  );
}
