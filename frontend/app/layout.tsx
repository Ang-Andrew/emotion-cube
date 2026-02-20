import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "emotion-cube — PS2 EE WASM PoC",
  description: "PS2 Emotion Engine proof-of-concept: Gouraud-shaded spinning cube via Rust/WASM + wgpu",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body style={{ margin: 0, background: "#000", color: "#0f0", fontFamily: "monospace" }}>
        {children}
      </body>
    </html>
  );
}
