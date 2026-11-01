import { lazy, Suspense, useState, useEffect } from "react";
import { KamishProvider } from "./context/KamishContext";
import { useKamish } from "./context/useKamish";
import DemoTasksProvider from "./context/DemoTasksProvider.jsx";
import Header from "./components/Header";
import Navbar from "./components/Navbar";

const HomePage = lazy(() => import("./pages/HomePage.jsx"));
const DashboardPage = lazy(() => import("./pages/DashboardPage.jsx"));
const TopicsPage = lazy(() => import("./pages/TopicsPage.jsx"));
const ProduceFetchPage = lazy(() => import("./pages/ProduceFetchPage.jsx"));
const AuthorityPage = lazy(() => import("./pages/AuthorityPage.jsx"));
const AuditVerificationPage = lazy(() => import("./pages/AuditVerificationPage.jsx"));
const SignInPage = lazy(() => import("./pages/SignInPage.jsx"));
const NotFoundPage = lazy(() => import("./pages/NotFoundPage.jsx"));

function parseHashRoute() {
  let hash = window.location.hash || "";
  if (hash.startsWith("#/")) {
    hash = hash.slice(2);
  } else if (hash.startsWith("#")) {
    hash = hash.slice(1);
  }
  
  if (!hash) {
    const path = window.location.pathname.replace(/^\//, "");
    if (path) {
      hash = path;
    }
  }

  const parts = hash.split("/").filter(Boolean);
  let route = parts[0] || "home";
  let param = parts[1] || null;
  try { param = param === null ? null : decodeURIComponent(param); }
  catch { route = "404"; param = null; }

  // Map backward compatibility aliases
  if (route === "tasks") route = "topics";
  if (route === "work") route = "console";
  if (route === "new") route = "authority";
  if (route === "contact") route = "audit";

  const validRoutes = ["home", "dashboard", "topics", "console", "authority", "audit", "signin", "404"];
  if (!validRoutes.includes(route)) route = "404";

  return { route, param };
}

function AppContent() {
  const [location, setLocation] = useState(parseHashRoute);
  const { isLiveMode, isSessionExpired } = useKamish();
  const currentRoute = location.route;

  useEffect(() => {
    const syncFromHash = () => setLocation(parseHashRoute());
    window.addEventListener("hashchange", syncFromHash);
    window.addEventListener("popstate", syncFromHash);

    return () => {
      window.removeEventListener("hashchange", syncFromHash);
      window.removeEventListener("popstate", syncFromHash);
    };
  }, []);

  const navigateTo = (route, param = null) => {
    let targetHash = "#/" + route;
    if (param) targetHash += "/" + encodeURIComponent(param);

    if (window.location.hash !== targetHash) {
      window.location.hash = targetHash;
    } else {
      setLocation(parseHashRoute());
    }
    window.scrollTo({ top: 0, behavior: "smooth" });
  };

  return (
    <div style={{ minHeight: "100vh", display: "flex", flexDirection: "column" }}>
      {/* Top Header with Brand, Live Status, 3D Story Link, Mode Toggle & User Controls */}
      <Header onNavigate={navigateTo} />

      {/* Prominent LIVE vs DEMO Mode Banner */}
      {isLiveMode ? (
        <div
          role="status"
          style={{
            padding: "0.5rem 2rem",
            textAlign: "center",
            background: "rgba(16, 185, 129, 0.12)",
            color: "#6ee7b7",
            borderBottom: "1px solid rgba(16, 185, 129, 0.25)",
            fontFamily: "var(--font-mono)",
            fontSize: "0.85rem",
          }}
        >
          ● LIVE BROKER OPERATION ACTIVE — Authenticated Rust Axum Gateway Connected (Port 3001)
        </div>
      ) : (
        <div
          role="status"
          style={{
            padding: "0.5rem 2rem",
            textAlign: "center",
            background: "rgba(245, 158, 11, 0.15)",
            color: "#fcd34d",
            borderBottom: "1px solid rgba(245, 158, 11, 0.25)",
            fontFamily: "var(--font-mono)",
            fontSize: "0.85rem",
          }}
        >
          ⚠ DEMO / SIMULATION MODE — Local Mock Simulation Active (Switch to LIVE in top bar)
        </div>
      )}

      {/* Session Expired Alert Banner */}
      {isSessionExpired && (
        <div
          role="alert"
          style={{
            padding: "0.75rem 2rem",
            textAlign: "center",
            background: "rgba(244, 63, 94, 0.2)",
            color: "#fda4af",
            borderBottom: "1px solid #f43f5e",
            fontFamily: "var(--font-mono)",
            fontSize: "0.85rem",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: "1rem",
          }}
        >
          <span>Session expired. Please sign in to resume authorized operations.</span>
          <button
            onClick={() => navigateTo("signin")}
            style={{
              background: "#f43f5e",
              color: "#fff",
              border: "none",
              padding: "3px 12px",
              borderRadius: "4px",
              cursor: "pointer",
              fontFamily: "var(--font-mono)",
              fontWeight: 700,
            }}
          >
            SIGN IN
          </button>
        </div>
      )}

      {/* Main Routed Content */}
      <main style={{ flex: 1 }}>
        <Suspense fallback={<p role="status" style={{ textAlign: "center", padding: "3rem", color: "#a855f7", fontFamily: "var(--font-mono)" }}>Loading view...</p>}>
          {currentRoute === "home" && <HomePage onNavigate={navigateTo} />}
          {currentRoute === "dashboard" && <DashboardPage onNavigate={navigateTo} />}
          {currentRoute === "topics" && <TopicsPage onNavigate={navigateTo} />}
          {currentRoute === "console" && <ProduceFetchPage onNavigate={navigateTo} />}
          {currentRoute === "authority" && <AuthorityPage onNavigate={navigateTo} />}
          {currentRoute === "audit" && <AuditVerificationPage onNavigate={navigateTo} />}
          {currentRoute === "signin" && <SignInPage onSignedIn={() => navigateTo("dashboard")} />}
          {currentRoute === "404" && <NotFoundPage onBack={() => navigateTo("home")} />}
        </Suspense>
      </main>

      {/* Bottom Floating Navigation Dock */}
      <Navbar currentRoute={currentRoute} onNavigate={navigateTo} />
    </div>
  );
}

export default function App() {
  return (
    <KamishProvider>
      <DemoTasksProvider>
        <AppContent />
      </DemoTasksProvider>
    </KamishProvider>
  );
}
