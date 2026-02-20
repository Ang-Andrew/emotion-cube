"use client";

import dynamic from "next/dynamic";

// EmulatorCanvas uses browser APIs (wasm, requestAnimationFrame) — disable SSR.
const EmulatorCanvas = dynamic(() => import("./EmulatorCanvas"), { ssr: false });

export default function Home() {
  return (
    <main
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        minHeight: "100vh",
        gap: "1rem",
      }}
    >
      <h1 style={{ fontSize: "1rem", letterSpacing: "0.1em", color: "#0f0", margin: 0 }}>
        emotion-cube / PS2 Emotion Engine PoC
      </h1>
      <EmulatorCanvas />
    </main>
  );
}
