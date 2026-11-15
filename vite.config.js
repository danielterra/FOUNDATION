import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import { isIP } from "node:net";
import { fileURLToPath } from "node:url";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
const projectRoot = fileURLToPath(new URL(".", import.meta.url));
const devInvokeTarget = "http://127.0.0.1:3030";
const allowedDevInvokeHost = "localhost:1420";
const allowedDevInvokeOrigin = "http://localhost:1420";

function isLoopbackAddress(address) {
  if (!address || isIP(address) === 0) {
    return false;
  }

  return address === "::1" || address === "127.0.0.1" || address === "::ffff:127.0.0.1";
}

function localDevInvokeGuard() {
  return {
    name: "local-dev-invoke-guard",
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const pathname = new URL(request.url ?? "/", "http://localhost").pathname;
        const isDevInvokePath = pathname === "/__dev-invoke" || pathname.startsWith("/__dev-invoke/");
        const isAssetPath = pathname === "/asset" || pathname.startsWith("/asset/");

        if (!isDevInvokePath && !isAssetPath) {
          return next();
        }

        const origin = request.headers.origin;
        const allowedOrigin = origin === undefined || origin === allowedDevInvokeOrigin;
        const allowedHost = request.headers.host === allowedDevInvokeHost;
        const allowedPeer = isLoopbackAddress(request.socket.remoteAddress);

        if (!allowedPeer || !allowedHost || !allowedOrigin) {
          response.statusCode = 403;
          response.end("Forbidden");
          return;
        }

        return next();
      });
    },
  };
}

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [localDevInvokeGuard(), tailwindcss(), sveltekit()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    proxy: {
      "/__dev-invoke": {
        target: devInvokeTarget,
        changeOrigin: false,
        rewrite: (path) => path.replace(/^\/__dev-invoke/, "") || "/",
      },
      "/asset": {
        target: devInvokeTarget,
        changeOrigin: false,
      },
    },
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
    fs: {
      allow: [projectRoot],
    },
  },
}));
