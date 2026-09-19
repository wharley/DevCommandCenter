interface ImportMetaEnv {
	readonly DEV: boolean;
	readonly PROD: boolean;
	readonly BASE_URL: string;
}

interface ImportMeta {
	readonly env: ImportMetaEnv;
}
