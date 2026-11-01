import React from "react";
import { useKamish } from "../context/useKamish";

export default function Header({ onNavigate }) {
  const {
    nodes,
    isLiveMode,
    toggleMode,
    currentUser,
    logoutFromGateway,
    gatewayStatus,
    isSessionExpired,
  } = useKamish();

  const activeRegions = Object.values(nodes).filter((node) => node.status === "HEALTHY").length;

  return (
    <header
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "1.25rem 2.5rem 0.5rem",
        background: "transparent",
        position: "relative",
        zIndex: 50,
        maxWidth: "1600px",
        margin: "0 auto",
        width: "100%",
        gap: "1rem",
        flexWrap: "wrap",
      }}
    >
      {/* Top Left: Brutalist Logo Box */}
      <div style={{ display: "flex", alignItems: "center", gap: "1.25rem", flexWrap: "wrap" }}>
        <button
          type="button"
          className="brutal-logo-box"
          onClick={() => onNavigate("home")}
          style={{
            cursor: "pointer",
            display: "inline-flex",
            alignItems: "center",
            gap: "10px",
            padding: "4px 16px 4px 8px",
          }}
        >
          <img
            src="/logo.png"
            alt="Kamish Dragon Logo"
            style={{
              width: "32px",
              height: "32px",
              objectFit: "contain",
              filter: "drop-shadow(0 0 8px rgba(192, 132, 252, 0.8))",
            }}
          />
          <span>KAMISH</span>
        </button>

        {/* Live Cluster Health Badge */}
        <div
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "8px",
            background: "rgba(15, 23, 42, 0.75)",
            border: "1px solid rgba(168, 85, 247, 0.3)",
            borderRadius: "9999px",
            padding: "5px 14px",
            fontSize: "0.85rem",
            fontFamily: "var(--font-mono)",
            color: "#e2e8f0",
          }}
        >
          <span
            style={{
              width: "8px",
              height: "8px",
              borderRadius: "50%",
              background: activeRegions > 0 ? "#10b981" : "#f43f5e",
              boxShadow: activeRegions > 0 ? "0 0 8px #10b981" : "0 0 8px #f43f5e",
              display: "inline-block",
            }}
          />
          <span>
            {activeRegions}/3 NODES ACTIVE · REGION: {gatewayStatus?.region || "EU_WEST"}
          </span>
        </div>

        {/* PROMINENT LIVE VS DEMO TOGGLE */}
        <div style={{ display: "inline-flex", alignItems: "center", gap: "8px" }}>
          <span
            id="mode-badge"
            style={{
              padding: "4px 10px",
              borderRadius: "4px",
              fontSize: "0.78rem",
              fontFamily: "var(--font-mono)",
              fontWeight: 800,
              display: "inline-flex",
              alignItems: "center",
              gap: "6px",
              background: isLiveMode ? "rgba(16, 185, 129, 0.2)" : "rgba(245, 158, 11, 0.2)",
              border: isLiveMode ? "1px solid #10b981" : "1px solid #f59e0b",
              color: isLiveMode ? "#6ee7b7" : "#fcd34d",
            }}
          >
            <span
              style={{
                width: "6px",
                height: "6px",
                borderRadius: "50%",
                background: isLiveMode ? "#10b981" : "#f59e0b",
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
              border: "1px solid rgba(255, 255, 255, 0.2)",
              color: "#cbd5e1",
              fontFamily: "var(--font-mono)",
              fontSize: "0.75rem",
              padding: "4px 10px",
              borderRadius: "4px",
              cursor: "pointer",
            }}
            title="Toggle between real broker gateway and local simulated demo"
          >
            SWITCH TO {isLiveMode ? "DEMO" : "LIVE"}
          </button>
        </div>
      </div>

      {/* Top Right: User & Navigation Controls */}
      <div style={{ display: "flex", alignItems: "center", gap: "1rem", flexWrap: "wrap" }}>
        <a
          href="/3d.html"
          target="_blank"
          rel="noopener noreferrer"
          style={{
            background: "rgba(255, 107, 53, 0.15)",
            backdropFilter: "blur(12px)",
            border: "1.5px solid rgba(255, 107, 53, 0.45)",
            color: "#ff6b35",
            fontFamily: "var(--font-mono)",
            fontSize: "0.85rem",
            fontWeight: 700,
            padding: "6px 16px",
            borderRadius: "9999px",
            textDecoration: "none",
            display: "inline-flex",
            alignItems: "center",
            gap: "6px",
          }}
        >
          3D Story ↗
        </a>

        {/* User Session Widget */}
        {currentUser ? (
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <span
              id="user-badge"
              style={{
                fontFamily: "var(--font-mono)",
                fontSize: "0.8rem",
                color: "#e9d5ff",
                background: "rgba(147, 51, 234, 0.25)",
                border: "1px solid #a855f7",
                padding: "4px 10px",
                borderRadius: "4px",
                display: "inline-flex",
                alignItems: "center",
                gap: "6px",
              }}
            >
              <span>{currentUser.username}</span>
              <span
                style={{
                  background: "#4c1d95",
                  color: "#d8b4fe",
                  fontSize: "0.7rem",
                  textTransform: "uppercase",
                  padding: "1px 6px",
                  borderRadius: "2px",
                  fontWeight: 700,
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
                border: "1px solid rgba(244, 63, 94, 0.5)",
                color: "#fda4af",
                fontFamily: "var(--font-mono)",
                fontSize: "0.75rem",
                padding: "4px 10px",
                borderRadius: "4px",
                cursor: "pointer",
              }}
            >
              Sign Out
            </button>
          </div>
        ) : (
          <button
            id="nav-signin-btn"
            onClick={() => onNavigate("signin")}
            style={{
              background: "rgba(168, 85, 247, 0.2)",
              border: "1.5px solid #a855f7",
              color: "#f3e8ff",
              fontFamily: "var(--font-mono)",
              fontSize: "0.85rem",
              fontWeight: 700,
              padding: "6px 18px",
              borderRadius: "9999px",
              cursor: "pointer",
            }}
          >
            Sign In
          </button>
        )}
      </div>
    </header>
  );
}
