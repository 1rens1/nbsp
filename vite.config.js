import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import process from "node:process";
import { readFileSync } from "node:fs";
const host = process.env.TAURI_DEV_HOST;

/**
 * A webview that cached a component from before its `<style>` block was removed still requests
 * `X.svelte?svelte&type=style&lang.css`. vite-plugin-svelte then falls back to the raw component
 * source, which Tailwind fails to parse ("Invalid declaration"). Serve empty CSS instead.
 */
/** @type {import("vite").Plugin} */
const staleSvelteCss = {
  name: "nbsp:stale-svelte-css",
  enforce: "pre",
  load(id) {
    if (!id.includes(".svelte?svelte&type=style")) return;
    try {
      if (!readFileSync(id.split("?")[0], "utf8").includes("<style")) return "";
    } catch {
      return "";
    }
  },
};

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [staleSvelteCss, tailwindcss(), sveltekit()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
