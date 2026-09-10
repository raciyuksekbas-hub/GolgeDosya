/// <reference types="vite/client" />

declare module "*?url" {
  const src: string;
  export default src;
}

/** Derleme zamanında `package.json` sürümünden yazılır (vite.config.ts). */
declare const __APP_VERSION__: string;
