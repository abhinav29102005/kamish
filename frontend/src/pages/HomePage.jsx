import React from "react";
import { lazy, Suspense } from "react";
const KamishDragon3D = lazy(() => import("../components/KamishDragon3D"));

export default function HomePage({ onNavigate }) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        height: "calc(100vh - 40px)",
        width: "100%",
        overflow: "hidden",
        position: "relative",
      }}
    >
      {/* Top: KAMISH Hollow Text with Balanced Letter-Spacing for Absolute Centering */}
      <div
        style={{
          paddingTop: "1.5rem",
          paddingBottom: "0.5rem",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: "100%",
          userSelect: "none",
          pointerEvents: "none",
          flexShrink: 0,
          zIndex: 1,
        }}
      >
        <h1
          style={{
            margin: 0,
            fontSize: "clamp(3.5rem, 8.5vw, 8.5rem)",
            fontWeight: 900,
            fontFamily: "var(--font-main)",
            letterSpacing: "0.22em",
            paddingLeft: "0.22em",
            color: "transparent",
            WebkitTextStroke: "2.5px rgba(192, 132, 252, 0.55)",
            textShadow: "0 0 50px rgba(168, 85, 247, 0.25)",
            textTransform: "uppercase",
            lineHeight: 1,
            textAlign: "center",
          }}
        >
          KAMISH
        </h1>
      </div>

      {/* Utmost Center: Remaining space between top text and bottom navbar */}
      <div
        style={{
          flex: 1,
          width: "100%",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          position: "relative",
          minHeight: 0,
          zIndex: 2,
        }}
      >
        <div
          style={{
            width: "100%",
            maxWidth: "1280px",
            height: "100%",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            position: "relative",
          }}
        >
          {typeof window !== "undefined" && window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches ? (
            <div
              style={{
                width: "100%",
                height: "100%",
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <img
                src="/logo.png"
                alt="Kamish Dragon Crest"
                style={{
                  width: "240px",
                  height: "240px",
                  objectFit: "contain",
                  filter: "drop-shadow(0 0 30px rgba(168, 85, 247, 0.7))",
                }}
              />
            </div>
          ) : (
            <Suspense
              fallback={
                <div
                  style={{
                    height: "100%",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    color: "#a855f7",
                    fontFamily: "var(--font-mono)",
                  }}
                >
                  INITIALIZING WEBGL 3D MESH...
                </div>
              }
            >
              <KamishDragon3D />
            </Suspense>
          )}
        </div>
      </div>
    </div>
  );
}
