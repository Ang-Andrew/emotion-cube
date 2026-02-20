"use client";

import { useEffect, useRef, useState } from "react";

const CANVAS_ID = "emotion-cube-canvas";

interface Telemetry {
  emulatedCycles: number;
  vu1MatOps: number;
  frameCount: number;
  hostFps: number;
}

export default function EmulatorCanvas() {
  const rafRef   = useRef<number | null>(null);
  const coreRef  = useRef<any>(null);
  const fpsAccum = useRef({ lastTime: 0, frames: 0, fps: 0 });

  const [telemetry, setTelemetry] = useState<Telemetry | null>(null);
  const [error,     setError]     = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function init() {
      try {
        // Load the wasm-pack JS glue from the public static path.
        // webpackIgnore skips bundling so import.meta.url resolves correctly
        // to /pkg/emotion_cube.js, letting the glue fetch emotion_cube_bg.wasm
        // from the same directory via the URL it computes at runtime.
        const wasm = await import(/* webpackIgnore: true */ "/pkg/emotion_cube.js");
        await (wasm as any).default(); // call the wasm init function

        if (cancelled) return;

        const { EmulatorCore } = wasm;
        const core = await EmulatorCore.create(CANVAS_ID);
        coreRef.current = core;

        // --- requestAnimationFrame loop (maps to PS2 VBLANK interrupt) ---
        function loop(timestamp: number) {
          if (cancelled) return;

          const acc = fpsAccum.current;
          acc.frames++;
          const elapsed = timestamp - acc.lastTime;
          if (elapsed >= 1000) {
            acc.fps      = Math.round((acc.frames * 1000) / elapsed);
            acc.frames   = 0;
            acc.lastTime = timestamp;
          }

          const t = core.step_frame() as any;
          setTelemetry({
            emulatedCycles: t.emulatedCycles,
            vu1MatOps:      t.vu1MatOps,
            frameCount:     t.frameCount,
            hostFps:        acc.fps,
          });

          rafRef.current = requestAnimationFrame(loop);
        }

        fpsAccum.current.lastTime = performance.now();
        rafRef.current = requestAnimationFrame(loop);
      } catch (e: any) {
        if (!cancelled) setError(String(e));
      }
    }

    init();

    return () => {
      cancelled = true;
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
    };
  }, []);

  return (
    <div style={{ position: "relative", display: "inline-block" }}>
      {/* PS2 native resolution: 640 × 448 */}
      <canvas
        id={CANVAS_ID}
        width={640}
        height={448}
        style={{ display: "block", border: "1px solid #333" }}
      />

      {/* Telemetry HUD — absolutely positioned overlay */}
      {telemetry && (
        <div
          style={{
            position: "absolute",
            top: 8,
            left: 8,
            background: "rgba(0,0,0,0.6)",
            color: "#0f0",
            fontFamily: "monospace",
            fontSize: "11px",
            lineHeight: "1.6",
            padding: "6px 10px",
            borderRadius: 4,
            pointerEvents: "none",
          }}
        >
          <div>Emulated Cycles/Frame: {(300_000).toLocaleString()}</div>
          <div>VU1 Matrix Ops: {telemetry.vu1MatOps.toLocaleString()}</div>
          <div>Host FPS: ~{telemetry.hostFps}</div>
        </div>
      )}

      {error && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            background: "rgba(0,0,0,0.85)",
            color: "#f44",
            fontFamily: "monospace",
            fontSize: "13px",
            padding: 16,
            textAlign: "center",
          }}
        >
          {error}
        </div>
      )}
    </div>
  );
}
