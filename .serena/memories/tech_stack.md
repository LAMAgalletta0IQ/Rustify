# Tech stack
- Svelte 5 runes + TypeScript 5 + Vite 6; no SvelteKit/router/Tailwind. npm lockfile committed.
- Tauri 2, Rust 2021 (minimum 1.82), Tokio, reqwest 0.12 native-tls.
- librespot 0.8.0 + librespot-oauth 0.8.0, default features disabled; native-tls + rodio-backend. Cargo.lock's vergen 9.0.6 pin is load-bearing.
- Windows WebView2 + rodio/WASAPI; release targets NSIS. Acrylic transparent undecorated window.
- Debug builds include tauri-plugin-mcp-bridge on loopback for Tauri MCP automation; never expose bridge in release.