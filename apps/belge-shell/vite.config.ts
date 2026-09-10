import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// Port 1421: bağımsız dört uygulama 1420 kullanıyor. Migration boyunca eski
// uygulama ile birleşik kabuk aynı anda çalıştırılabilmeli.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1421, strictPort: true },
  build: { target: "safari15", sourcemap: false },
});
