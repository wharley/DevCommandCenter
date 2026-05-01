import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type AppUpdateInfo =
	| {
			stage: "downloaded";
			currentVersion: string;
			version: string;
			body?: string | null;
			date?: string | null;
	  }
	| null;

type AppCheckUpdateResponse =
	| {
			available: true;
			currentVersion: string;
			version: string;
			body?: string | null;
			date?: string | null;
	  }
	| {
			available: false;
			currentVersion: string;
			checkError?: string | null;
	  };

export function useAppUpdate() {
	const [update, setUpdate] = useState<AppUpdateInfo>(null);
	const [isChecking, setIsChecking] = useState(false);
	const [isInstalling, setIsInstalling] = useState(false);

	const checkForUpdate = useCallback(async () => {
		setIsChecking(true);
		try {
			const result = await invoke<AppCheckUpdateResponse>("app_check_for_updates");
			if (result.available) {
				setUpdate({
					stage: "downloaded",
					currentVersion: result.currentVersion,
					version: result.version,
					body: result.body ?? null,
					date: result.date ?? null,
				});
			} else {
				setUpdate(null);
			}
		} catch {
			setUpdate(null);
		} finally {
			setIsChecking(false);
		}
	}, []);

	const installUpdate = useCallback(async () => {
		if (!update) {
			return;
		}

		setIsInstalling(true);
		try {
			await invoke<{ success: true }>("app_quit_and_install");
		} finally {
			setIsInstalling(false);
		}
	}, [update]);

	useEffect(() => {
		void checkForUpdate();
	}, [checkForUpdate]);

	return {
		update,
		isChecking,
		isInstalling,
		checkForUpdate,
		installUpdate,
	};
}
