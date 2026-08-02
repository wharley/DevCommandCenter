import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import {
	automaticUpdateCheckIsDue,
	UPDATE_CHECK_INTERVAL_MS,
} from "./update-check-policy";

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
	const checkInFlightRef = useRef<Promise<void> | null>(null);
	const lastCheckStartedAtRef = useRef(0);

	const runUpdateCheck = useCallback((force = false) => {
		const now = Date.now();
		if (checkInFlightRef.current) {
			return checkInFlightRef.current;
		}
		if (!force && !automaticUpdateCheckIsDue(lastCheckStartedAtRef.current, now)) {
			return Promise.resolve();
		}

		lastCheckStartedAtRef.current = now;
		setIsChecking(true);
		const request = Promise.resolve().then(async () => {
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
				} else if (result.checkError) {
					// A transient provider failure must not hide an update already discovered.
					setCheckError(result.checkError);
				} else {
					setUpdate(null);
					setCheckError(null);
				}
			} catch (error) {
				// Keep a known update visible when a later background refresh fails.
				setCheckError(error instanceof Error ? error.message : String(error));
			} finally {
				checkInFlightRef.current = null;
				setIsChecking(false);
			}
		});
		checkInFlightRef.current = request;
		return request;
	}, []);

	const checkForUpdate = useCallback(
		() => runUpdateCheck(true),
		[runUpdateCheck],
	);

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
		void runUpdateCheck(true);
		const checkWhenActive = () => {
			if (document.visibilityState === "visible") {
				void runUpdateCheck();
			}
		};
		const intervalId = window.setInterval(checkWhenActive, UPDATE_CHECK_INTERVAL_MS);
		window.addEventListener("focus", checkWhenActive);
		document.addEventListener("visibilitychange", checkWhenActive);

		return () => {
			window.clearInterval(intervalId);
			window.removeEventListener("focus", checkWhenActive);
			document.removeEventListener("visibilitychange", checkWhenActive);
		};
	}, [runUpdateCheck]);

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
