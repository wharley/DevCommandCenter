import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";

export type AppUpdateInfo =
	| {
			stage: "available";
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
	const { t } = useTranslation("common");
	const [update, setUpdate] = useState<AppUpdateInfo>(null);
	const [currentVersion, setCurrentVersion] = useState<string | null>(null);
	const [checkError, setCheckError] = useState<string | null>(null);
	const [isChecking, setIsChecking] = useState(false);
	const [isInstalling, setIsInstalling] = useState(false);

	const checkForUpdate = useCallback(async () => {
		setIsChecking(true);
		try {
			const result = await invoke<AppCheckUpdateResponse>("app_check_for_updates");
			setCurrentVersion(result.currentVersion);
			if (result.available) {
				setCheckError(null);
				setUpdate({
					stage: "available",
					currentVersion: result.currentVersion,
					version: result.version,
					body: result.body ?? null,
					date: result.date ?? null,
				});
			} else {
				setUpdate(null);
				setCheckError(result.checkError ?? null);
			}
		} catch (error) {
			setUpdate(null);
			setCheckError(error instanceof Error ? error.message : String(error));
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
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			toast.error(t("updater.installFailed", { message }));
			throw error;
		} finally {
			setIsInstalling(false);
		}
	}, [t, update]);

	useEffect(() => {
		void checkForUpdate();
	}, [checkForUpdate]);

	return {
		update,
		currentVersion,
		checkError,
		isChecking,
		isInstalling,
		checkForUpdate,
		installUpdate,
	};
}
