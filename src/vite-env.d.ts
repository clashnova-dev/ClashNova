/// <reference types="vite/client" />
interface ImportMetaEnv {
  readonly VITE_DEFAULT_SERVER?: string;
  readonly VITE_ENABLE_LOGIN?: string;
  readonly VITE_GITHUB_URL?: string;
}
interface ImportMeta {
  readonly env: ImportMetaEnv;
}
