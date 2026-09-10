import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";

// Sürüm tek yerden: package.json. Açılış ekranı sürümü IPC beklemeden
// gösterebilsin diye derleme zamanında gömülür.
const { version } = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8"));

// Port 1421: bağımsız dört uygulama 1420 kullanıyor. Migration boyunca eski
// uygulama ile birleşik kabuk aynı anda çalıştırılabilmeli.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  define: { __APP_VERSION__: JSON.stringify(version) },
  server: { port: 1421, strictPort: true },
  build: { target: "safari15", sourcemap: false },
});
