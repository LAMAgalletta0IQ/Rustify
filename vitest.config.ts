import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Separate from vite.config.ts because Tauri's CLI reads that file directly
// (via `beforeDevCommand`/`beforeBuildCommand`) and a `test` block there is
// dead weight for every dev/build invocation. Shares the same svelte()
// plugin so `.svelte.ts` rune modules like store.svelte.ts compile the same
// way under test as they do in the real app.
export default defineConfig({
  plugins: [svelte()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});
