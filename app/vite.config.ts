import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Local port 3000 is already in use on this machine by another process, so
// the dev server defaults to Vite's standard 5173 instead.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    strictPort: true,
    // `tauri dev` builds the Rust shell in src-tauri/target/ while this dev
    // server is running. Vite's default watcher otherwise picks up churn in
    // there (including a build-script binary that's mid-write / locked by
    // cargo), and on Windows that intermittently throws an EBUSY error that
    // crashes the whole `beforeDevCommand` step instead of just logging a
    // warning. None of that directory is frontend source, so ignore it
    // wholesale.
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  clearScreen: false,
});
