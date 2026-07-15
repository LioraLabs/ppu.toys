/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_MSW?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
