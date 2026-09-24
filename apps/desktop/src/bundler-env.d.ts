interface ImportMetaEnv {
	readonly DEV: boolean;
	readonly PROD: boolean;
	readonly BASE_URL: string;
	/** Dev-only: force | skip. */
	readonly VITE_DEV_ONBOARDING?: string;
}

interface ImportMeta {
	readonly env: ImportMetaEnv;
	readonly hot?: {
		dispose(callback: () => void): void;
		accept(
			dependencies: string[],
			callback: (modules: Array<{ default: Record<string, unknown> } | undefined>) => void,
		): void;
	};
}

declare module "*.css" {
	const content: string;
	export default content;
}

declare module "*?inline" {
	const content: string;
	export default content;
}

declare module "*?raw" {
	const content: string;
	export default content;
}

declare module "*?worker" {
	const WorkerFactory: new () => Worker;
	export default WorkerFactory;
}
