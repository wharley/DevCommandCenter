/// <reference types="vite/client" />

interface ImportMetaEnv {
	/** Dev-only: force | skip (see `.env.example`). */
	readonly VITE_DEV_ONBOARDING?: string;
}

interface ImportMeta {
	readonly env: ImportMetaEnv;
}

declare module "*?worker" {
	const WorkerFactory: new () => Worker;
	export default WorkerFactory;
}
