import React, { useState } from "react";
import { useKamish } from "../context/useKamish";

export default function Navbar({ currentRoute, onNavigate }) {
  const [showSettings, setShowSettings] = useState(false);
  const {
    nodes,
    isLiveMode,
    toggleMode,
    connectionStatus,
    currentUser,
    logoutFromGateway,
    gatewayStatus,
  } = useKamish();

  const activeRegions = Object.values(nodes).filter((node) => node.status === "HEALTHY").length;

  const navItems = [
    { id: "home", label: "Home" },
    { id: "dashboard", label: "Dashboard" },
    { id: "topics", label: "Topics" },
    { id: "console", label: "Produce/Fetch" },
    { id: "authority", label: "Authority" },
    { id: "audit", label: "Audit" },
  ];

  return (
    <>
      {/* Transparent backdrop overlay to dismiss drawer on outside click */}
      {showSettings && (
        <div
          onClick={() => setShowSettings(false)}
          style={{
            position: "fixed",
            inset: 0,
            zIndex: 105,
            background: "transparent",
          }}
        />
      )}

      {/* Small Settings Drawer / Popover anchored above the settings button */}
      {showSettings && (
        <div
          id="settings-drawer"
          style={{
            position: "fixed",
            bottom: "48px",
            left: "50%",
            transform: "translateX(-50%)",
            width: "360px",
            maxWidth: "92vw",
            background: "rgba(12, 6, 24, 0.97)",
            backdropFilter: "blur(20px)",
            WebkitBackdropFilter: "blur(20px)",
            border: "1.5px solid rgba(168, 85, 247, 0.4)",
            boxShadow: "0 12px 36px rgba(0, 0, 0, 0.85), 0 0 24px rgba(168, 85, 247, 0.25)",
            zIndex: 120,
            padding: "1rem 1.25rem",
            fontFamily: "var(--font-mono)",
            borderRadius: "2px",
            animation: "fadeIn 0.15s ease",
          }}
        >
          {/* Settings Header */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              borderBottom: "1px solid rgba(168, 85, 247, 0.25)",
              paddingBottom: "0.5rem",
              marginBottom: "0.85rem",
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: "6px", color: "#c084fc", fontWeight: 800, fontSize: "0.78rem", letterSpacing: "0.08em" }}>
              <span>⚙</span>
              <span>CONSOLE SETTINGS</span>
            </div>
            <button
              onClick={() => setShowSettings(false)}
              style={{
                background: "transparent",
                border: "none",
                color: "#94a3b8",
                cursor: "pointer",
                fontSize: "0.85rem",
                padding: "2px 6px",
              }}
              title="Close Settings"
            >
              ✕
            </button>
          </div>

          {/* 1. Mode Switcher Section */}
          <div style={{ marginBottom: "0.85rem" }}>
            <div style={{ fontSize: "0.68rem", color: "#94a3b8", letterSpacing: "0.06em", marginBottom: "0.4rem", textTransform: "uppercase" }}>
              Cluster Connection Mode
            </div>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "8px" }}>
              <span
                id="mode-badge"
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: "6px",
                  padding: "4px 10px",
                  fontSize: "0.74rem",
                  fontWeight: 800,
                  letterSpacing: "0.04em",
                  color: isLiveMode ? "#4ade80" : "#facc15",
                  background: isLiveMode ? "rgba(16, 185, 129, 0.15)" : "rgba(245, 158, 11, 0.15)",
                  border: isLiveMode ? "1px solid rgba(16, 185, 129, 0.4)" : "1px solid rgba(245, 158, 11, 0.4)",
                  borderRadius: "2px",
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
                  background: "rgba(255, 255, 255, 0.05)",
                  border: "1px solid rgba(168, 85, 247, 0.35)",
                  color: "#e2e8f0",
                  fontFamily: "inherit",
                  fontSize: "0.72rem",
                  fontWeight: 700,
                  padding: "4px 10px",
                  borderRadius: "2px",
                  cursor: "pointer",
                  letterSpacing: "0.05em",
                  transition: "all 0.15s ease",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = "#c084fc";
                  e.currentTarget.style.color = "#0a0513";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background = "rgba(255, 255, 255, 0.05)";
                  e.currentTarget.style.color = "#e2e8f0";
                }}
              >
                {isLiveMode ? "SWITCH TO DEMO" : "SWITCH TO LIVE"}
              </button>
            </div>
          </div>

          {/* 2. Cluster Telemetry Status */}
          <div
            style={{
              padding: "0.5rem 0.65rem",
              background: "rgba(0, 0, 0, 0.4)",
              border: "1px solid rgba(168, 85, 247, 0.2)",
              borderRadius: "2px",
              marginBottom: "0.85rem",
              fontSize: "0.72rem",
              color: "#cbd5e1",
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            <span>GATEWAY / CLUSTER</span>
            <span
              style={{
                color: isLiveMode
                  ? connectionStatus === "connected"
                    ? "#4ade80"
                    : connectionStatus === "connecting"
                    ? "#facc15"
                    : "#f43f5e"
                  : "#facc15",
                fontWeight: 700,
              }}
            >
              {isLiveMode
                ? connectionStatus === "connected"
                  ? `CONNECTED · ${gatewayStatus?.region || "EU_WEST"}`
                  : connectionStatus.toUpperCase()
                : `${activeRegions}/3 NODES (DEMO)`}
            </span>
          </div>

          {/* 3. Authentication & RBAC Session */}
          <div style={{ borderTop: "1px solid rgba(168, 85, 247, 0.25)", paddingTop: "0.75rem" }}>
            <div style={{ fontSize: "0.68rem", color: "#94a3b8", letterSpacing: "0.06em", marginBottom: "0.4rem", textTransform: "uppercase" }}>
              Authentication & Role
            </div>
            {currentUser ? (
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                <span
                  id="user-badge"
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: "6px",
                    padding: "3px 8px",
                    fontSize: "0.74rem",
                    color: "#e9d5ff",
                    background: "rgba(147, 51, 234, 0.2)",
                    border: "1px solid rgba(168, 85, 247, 0.35)",
                    borderRadius: "2px",
                    fontWeight: 700,
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
                    border: "1px solid rgba(244, 63, 94, 0.4)",
                    color: "#fda4af",
                    fontFamily: "inherit",
                    fontSize: "0.7rem",
                    fontWeight: 700,
                    padding: "3px 10px",
                    borderRadius: "2px",
                    cursor: "pointer",
                    transition: "all 0.15s ease",
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = "rgba(244, 63, 94, 0.25)";
                    e.currentTarget.style.color = "#ffffff";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = "transparent";
                    e.currentTarget.style.color = "#fda4af";
                  }}
                >
                  SIGN OUT
                </button>
              </div>
            ) : (
              <button
                id="nav-signin-btn"
                onClick={() => {
                  onNavigate("signin");
                  setShowSettings(false);
                }}
                style={{
                  width: "100%",
                  background: "rgba(168, 85, 247, 0.2)",
                  border: "1px solid #c084fc",
                  color: "#f3e8ff",
                  fontFamily: "inherit",
                  fontSize: "0.74rem",
                  fontWeight: 700,
                  padding: "6px 12px",
                  borderRadius: "2px",
                  cursor: "pointer",
                  letterSpacing: "0.06em",
                  transition: "all 0.15s ease",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = "#c084fc";
                  e.currentTarget.style.color = "#0a0513";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background = "rgba(168, 85, 247, 0.2)";
                  e.currentTarget.style.color = "#f3e8ff";
                }}
              >
                SIGN IN / AUTHENTICATE →
              </button>
            )}
          </div>
        </div>
      )}

      {/* Clean, Small, Connected Navbar (NO PILLS, NO SIGN IN) */}
      <header
        role="banner"
        style={{
          position: "fixed",
          bottom: 0,
          left: 0,
          right: 0,
          zIndex: 120,
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
          {/* Navigation Tabs (Contiguous Connected Strip) */}
          <nav
            aria-label="Main Navigation"
            style={{
              display: "inline-flex",
              alignItems: "stretch",
            }}
          >
            {navItems.map((item) => {
              const isActive = currentRoute === item.id;
              return (
                <button
                  key={item.id}
                  id={`nav-${item.id}`}
                  onClick={() => { onNavigate(item.id); setShowSettings(false); }}
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
                    padding: "0 18px",
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

          {/* Small Connected Settings Trigger */}
          <button
            id="settings-btn"
            onClick={() => setShowSettings(!showSettings)}
            style={{
              background: showSettings ? "#c084fc" : "transparent",
              color: showSettings ? "#0a0513" : "#cbd5e1",
              border: "none",
              fontFamily: "var(--font-mono)",
              fontSize: "0.76rem",
              fontWeight: 700,
              letterSpacing: "0.06em",
              textTransform: "uppercase",
              padding: "0 14px",
              height: "100%",
              cursor: "pointer",
              transition: "all 0.15s ease",
              borderRadius: "0px",
              display: "inline-flex",
              alignItems: "center",
              gap: "6px",
              whiteSpace: "nowrap",
            }}
            onMouseEnter={(e) => {
              if (!showSettings) {
                e.currentTarget.style.background = "rgba(168, 85, 247, 0.15)";
                e.currentTarget.style.color = "#ffffff";
              }
            }}
            onMouseLeave={(e) => {
              if (!showSettings) {
                e.currentTarget.style.background = "transparent";
                e.currentTarget.style.color = "#cbd5e1";
              }
            }}
            title="Cluster & Broker Settings"
          >
            <span>⚙</span>
            <span>SETTINGS</span>
          </button>
        </div>
      </header>
    </>
  );
}
