import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Local port 3000 is already in use on this machine by another process, so
// the dev server defaults to Vite's standard 5173 instead.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    strictPort: true,
  },
  clearScreen: false,
});
