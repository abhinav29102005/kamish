import React, { useState } from "react";
import { useKamish } from "../context/useKamish";

export default function SignInPage({ onSignedIn }) {
  const { currentUser, loginToGateway, logoutFromGateway, isSessionExpired } = useKamish();

  const isDevLoginEnabled = Boolean(
    import.meta.env.DEV || import.meta.env.VITE_ENABLE_DEV_LOGIN === "true"
  );

  const [username, setUsername] = useState(isDevLoginEnabled ? "admin" : "");
  const [password, setPassword] = useState(isDevLoginEnabled ? "kamish-admin-2026" : "");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  const handleSubmit = async (e) => {
    if (e) e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      await loginToGateway(username, password);
      if (onSignedIn) onSignedIn();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Authentication failed");
    } finally {
      setLoading(false);
    }
  };

  const handleQuickFill = (u, p) => {
    setUsername(u);
    setPassword(p);
    setError(null);
  };

  if (currentUser) {
    return (
      <section style={{ maxWidth: "680px", margin: "0 auto", padding: "3rem 2rem 120px", textAlign: "center" }}>
        <img
          src="/logo.png"
          alt="Kamish Dragon Logo"
          style={{
            width: "96px",
            height: "96px",
            objectFit: "contain",
            marginBottom: "1rem",
            filter: "drop-shadow(0 0 16px rgba(192, 132, 252, 0.6))",
          }}
        />
        <h1 style={{ fontFamily: "var(--font-mono)", fontSize: "2rem", textTransform: "uppercase" }}>
          AUTHENTICATED SESSION ACTIVE
        </h1>
        <div className="brutal-card-offset" style={{ padding: "2.5rem", textAlign: "left", marginTop: "2rem" }}>
          <p style={{ fontFamily: "var(--font-mono)", fontSize: "1.1rem" }}>
            Signed in as: <strong style={{ color: "#c084fc" }}>{currentUser.username}</strong>
          </p>
          <p style={{ fontFamily: "var(--font-mono)" }}>
            Assigned Role: <span style={{ textTransform: "uppercase", background: "#4c1d95", color: "#e9d5ff", padding: "3px 10px", borderRadius: "4px" }}>{currentUser.role}</span>
          </p>
          <div style={{ display: "flex", gap: "1rem", marginTop: "2rem" }}>
            <button className="brutal-btn-primary" id="goto-dashboard-btn" onClick={onSignedIn}>
              GO TO DASHBOARD
            </button>
            <button
              onClick={logoutFromGateway}
              style={{
                background: "transparent",
                border: "2px solid #f43f5e",
                color: "#fda4af",
                padding: "0.75rem 1.5rem",
                fontFamily: "var(--font-mono)",
                cursor: "pointer",
                fontWeight: 700,
              }}
            >
              SIGN OUT
            </button>
          </div>
        </div>
      </section>
    );
  }

  return (
    <section style={{ maxWidth: "680px", margin: "0 auto", padding: "2.5rem 2rem 120px", textAlign: "center" }}>
      <img
        src="/logo.png"
        alt="Kamish Dragon Logo"
        style={{
          width: "96px",
          height: "96px",
          objectFit: "contain",
          marginBottom: "1rem",
          filter: "drop-shadow(0 0 16px rgba(192, 132, 252, 0.6))",
        }}
      />
      <h1 style={{ fontFamily: "var(--font-mono)", fontSize: "2.2rem", textTransform: "uppercase" }}>
        GATEWAY LOGIN
      </h1>
      <p style={{ color: "#a855f7", fontFamily: "var(--font-mono)", fontSize: "0.95rem" }}>
        PBKDF2-HMAC-SHA256 Mutual Auth · Session Protected with CSRF
      </p>

      {isSessionExpired && (
        <div style={{ background: "rgba(244, 63, 94, 0.15)", border: "1px solid #f43f5e", color: "#fda4af", padding: "1rem", margin: "1rem 0", borderRadius: "8px", fontFamily: "var(--font-mono)", fontSize: "0.9rem" }}>
          ⚠ Your session has expired. Please sign in again.
        </div>
      )}

      {error && (
        <div role="alert" style={{ background: "rgba(244, 63, 94, 0.2)", border: "1px solid #f43f5e", color: "#fda4af", padding: "1rem", margin: "1rem 0", borderRadius: "8px", fontFamily: "var(--font-mono)", fontSize: "0.9rem" }}>
          {error}
        </div>
      )}

      <div className="brutal-card-offset" style={{ padding: "2.5rem", textAlign: "left", marginTop: "1.5rem" }}>
        <form onSubmit={handleSubmit}>
          <div style={{ marginBottom: "1.25rem" }}>
            <label htmlFor="username" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.85rem", color: "#d8b4fe", marginBottom: "0.5rem" }}>
              USERNAME
            </label>
            <input
              id="username"
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="Enter operator username"
              required
              autoComplete="username"
              style={{
                width: "100%",
                padding: "0.75rem 1rem",
                background: "#0d071a",
                border: "1.5px solid #a855f7",
                color: "#ffffff",
                fontFamily: "var(--font-mono)",
                fontSize: "1rem",
                borderRadius: "4px",
                outline: "none",
              }}
            />
          </div>

          <div style={{ marginBottom: "1.5rem" }}>
            <label htmlFor="password" style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.85rem", color: "#d8b4fe", marginBottom: "0.5rem" }}>
              PASSWORD
            </label>
            <input
              id="password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="Enter operator password"
              required
              autoComplete="current-password"
              style={{
                width: "100%",
                padding: "0.75rem 1rem",
                background: "#0d071a",
                border: "1.5px solid #a855f7",
                color: "#ffffff",
                fontFamily: "var(--font-mono)",
                fontSize: "1rem",
                borderRadius: "4px",
                outline: "none",
              }}
            />
          </div>

          <button
            id="signin-btn"
            type="submit"
            disabled={loading}
            className="brutal-btn-primary"
            style={{ width: "100%", padding: "1rem", fontSize: "1.1rem", opacity: loading ? 0.7 : 1 }}
          >
            {loading ? "AUTHENTICATING..." : "SIGN IN TO BROKER GATEWAY"}
          </button>
        </form>

        {isDevLoginEnabled && (
          <div style={{ marginTop: "2rem", borderTop: "1px solid rgba(255,255,255,0.1)", paddingTop: "1.5rem" }}>
            <span style={{ display: "block", fontFamily: "var(--font-mono)", fontSize: "0.8rem", color: "#fbbf24", marginBottom: "0.75rem" }}>
              DEVELOPMENT PRESET CREDENTIALS (VITE_ENABLE_DEV_LOGIN=true):
            </span>
            <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem" }}>
              {[
                { role: "Admin", u: "admin", p: "kamish-admin-2026" },
                { role: "Operator", u: "operator", p: "kamish-operator-2026" },
                { role: "Auditor", u: "auditor", p: "kamish-auditor-2026" },
                { role: "Producer", u: "producer", p: "kamish-producer-2026" },
                { role: "Consumer", u: "consumer", p: "kamish-consumer-2026" },
              ].map(({ role, u, p }) => (
                <button
                  key={role}
                  type="button"
                  onClick={() => handleQuickFill(u, p)}
                  style={{
                    background: "rgba(168, 85, 247, 0.15)",
                    border: "1px solid rgba(168, 85, 247, 0.4)",
                    color: "#d8b4fe",
                    padding: "4px 10px",
                    borderRadius: "4px",
                    fontSize: "0.75rem",
                    fontFamily: "var(--font-mono)",
                    cursor: "pointer",
                  }}
                >
                  {role} ({u})
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
