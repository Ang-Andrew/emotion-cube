import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  webpack: (config) => {
    // Required for loading the wasm-pack output as an async WebAssembly module.
    config.experiments = {
      asyncWebAssembly: true,
      layers: true,
    };
    return config;
  },
};

export default nextConfig;
