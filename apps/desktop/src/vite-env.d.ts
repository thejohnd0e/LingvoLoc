/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_BUILD_NUMBER: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
