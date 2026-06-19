import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

// Tauri's embedded-asset protocol can fail to load module scripts that carry a
// `crossorigin` attribute, leaving a blank window in the built app (works in dev
// because that loads over http). Strip it from the generated index.html.
function stripCrossorigin(): Plugin {
  return {
    name: "strip-crossorigin",
    transformIndexHtml(html) {
      return html.replace(/\s+crossorigin/g, "");
    },
  };
}

// Tauri expects a fixed port and serves the frontend from here in dev.
export default defineConfig({
  plugins: [react(), stripCrossorigin()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Don't watch the Rust side from Vite.
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "esnext",
    outDir: "dist",
  },
});
