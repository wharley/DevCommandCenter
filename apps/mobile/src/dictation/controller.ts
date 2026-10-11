/**
 * Dictation into a composer with the phone's own recognizer
 * (`expo-speech-recognition`: SFSpeechRecognizer on iOS, SpeechRecognizer on
 * Android), in pt-BR and on the device when it can. A tap starts, the text
 * shows up live after what was typed, another tap stops; nothing is ever sent
 * from here. The audio never reaches the desktop: only the text the person
 * then sends does.
 *
 * Kept free of React and of the native module (injected) so the permission
 * and state flow runs in tests.
 */
import {
	DICTATION_LOCALE,
	dictationErrorMessage,
	dictationResult,
	dictationText,
	hasOfflineLocale,
	startDictation,
	type DictationDraft,
} from "@dcc/mobile-core";

export type SpeechPermission = { granted: boolean; canAskAgain: boolean };

/** The slice of `ExpoSpeechRecognitionModule` used here. */
export type SpeechModule = {
	isRecognitionAvailable(): boolean;
	supportsOnDeviceRecognition(): boolean;
	getSupportedLocales(options: { androidRecognitionServicePackage?: string }): Promise<{ installedLocales: string[] }>;
	getPermissionsAsync(): Promise<SpeechPermission>;
	requestPermissionsAsync(): Promise<SpeechPermission>;
	start(options: Record<string, unknown>): void;
	stop(): void;
	abort(): void;
};

export type DictationPhase = "idle" | "preparing" | "listening" | "stopping";

export type DictationState = {
	phase: DictationPhase;
	error: string | null;
	/** The error is a permission the person has to give in the system settings. */
	needsSettings: boolean;
	/** Recognition runs without the network. */
	onDevice: boolean;
};

const IDLE: DictationState = { phase: "idle", error: null, needsSettings: false, onDevice: false };

/** Android's on-device recognizer (Google's "Android System Intelligence"). */
const ANDROID_ON_DEVICE_SERVICE = "com.google.android.as";
const STOP_GRACE_MS = 3_000;

export type DictationOptions = {
	module: SpeechModule;
	platform: "ios" | "android" | "web";
	/** The field's current text, read when dictation starts. */
	getText: () => string;
	/** Called with the field's new text on every result. */
	setText: (text: string) => void;
	onState: (state: DictationState) => void;
};

export class DictationController {
	private state: DictationState = IDLE;
	private draft: DictationDraft | null = null;
	/** This controller started the running session (events are global). */
	private active = false;
	private fellBack = false;
	private stopTimer: ReturnType<typeof setTimeout> | null = null;

	constructor(private readonly options: DictationOptions) {}

	available(): boolean {
		try {
			return this.options.module.isRecognitionAvailable();
		} catch {
			return false;
		}
	}

	current(): DictationState {
		return this.state;
	}

	toggle(): Promise<void> {
		if (this.state.phase === "idle") return this.start();
		this.stop();
		return Promise.resolve();
	}

	async start(): Promise<void> {
		if (this.state.phase !== "idle") return;
		this.set({ ...IDLE, phase: "preparing" });
		const { module } = this.options;
		try {
			let permission = await module.getPermissionsAsync();
			// Asked only the first time; after a refusal only Settings can change it.
			if (!permission.granted && permission.canAskAgain) permission = await module.requestPermissionsAsync();
			if (!permission.granted) {
				this.set({ ...IDLE, error: dictationErrorMessage("not-allowed"), needsSettings: true });
				return;
			}
			if (!this.available()) {
				this.set({ ...IDLE, error: dictationErrorMessage("service-not-allowed") });
				return;
			}
			const onDevice = await this.canRunOnDevice();
			this.draft = startDictation(this.options.getText());
			this.active = true;
			this.fellBack = false;
			this.relaunchOnEnd = false;
			this.launch(onDevice);
		} catch (error) {
			this.active = false;
			this.set({ ...IDLE, error: error instanceof Error && error.message ? error.message : dictationErrorMessage("unknown") });
		}
	}

	stop(): void {
		if (!this.active || this.state.phase === "idle") return;
		this.set({ ...this.state, phase: "stopping" });
		try {
			this.options.module.stop();
		} catch {
			this.finish();
			return;
		}
		// The recognizer ends with `end`; never leave the button stuck if it doesn't.
		this.clearStopTimer();
		this.stopTimer = setTimeout(() => {
			if (!this.active) return;
			try {
				this.options.module.abort();
			} catch {}
			this.finish();
		}, STOP_GRACE_MS);
	}

	/** The screen goes away mid-dictation. */
	dispose(): void {
		this.clearStopTimer();
		if (this.active) {
			try {
				this.options.module.abort();
			} catch {}
		}
		this.active = false;
		this.draft = null;
	}

	// ---- native events (forwarded by the hook) ----

	handleStart(): void {
		if (this.active && this.state.phase === "preparing") this.set({ ...this.state, phase: "listening" });
	}

	handleResult(transcript: string, isFinal: boolean): void {
		if (!this.active || !this.draft) return;
		this.draft = dictationResult(this.draft, transcript, isFinal);
		this.options.setText(dictationText(this.draft));
	}

	handleError(code: string): void {
		if (!this.active) return;
		// On-device recognition without the pt-BR model: once this attempt
		// ends, try again through the platform's regular recognizer.
		if (code === "language-not-supported" && this.state.onDevice && !this.fellBack) {
			this.fellBack = true;
			this.relaunchOnEnd = true;
			return;
		}
		const message = dictationErrorMessage(code);
		if (message) this.set({ ...this.state, error: message, needsSettings: code === "not-allowed" });
	}

	handleEnd(): void {
		if (!this.active) return;
		if (this.relaunchOnEnd) {
			this.relaunchOnEnd = false;
			this.launch(false);
			return;
		}
		this.finish();
	}

	private relaunchOnEnd = false;

	private launch(onDevice: boolean): void {
		this.set({ ...this.state, phase: "preparing", error: null, onDevice });
		try {
			this.options.module.start({
				lang: DICTATION_LOCALE,
				interimResults: true,
				// Runs until the person taps again (Android 13+ and iOS).
				continuous: true,
				requiresOnDeviceRecognition: onDevice,
				addsPunctuation: true,
				iosTaskHint: "dictation",
				...(this.options.platform === "android" && onDevice
					? { androidRecognitionServicePackage: ANDROID_ON_DEVICE_SERVICE }
					: {}),
			});
		} catch (error) {
			this.active = false;
			this.set({ ...IDLE, error: error instanceof Error && error.message ? error.message : dictationErrorMessage("unknown") });
		}
	}

	private async canRunOnDevice(): Promise<boolean> {
		const { module, platform } = this.options;
		try {
			if (!module.supportsOnDeviceRecognition()) return false;
			// iOS applies it only when the pt-BR recognizer supports it.
			if (platform === "ios") return true;
			if (platform !== "android") return false;
			const { installedLocales } = await module.getSupportedLocales({
				androidRecognitionServicePackage: ANDROID_ON_DEVICE_SERVICE,
			});
			return hasOfflineLocale(installedLocales);
		} catch {
			return false;
		}
	}

	private finish(): void {
		this.clearStopTimer();
		this.active = false;
		this.draft = null;
		this.set({ ...this.state, phase: "idle" });
	}

	private clearStopTimer() {
		if (this.stopTimer) clearTimeout(this.stopTimer);
		this.stopTimer = null;
	}

	private set(state: DictationState) {
		this.state = state;
		this.options.onState(state);
	}
}
