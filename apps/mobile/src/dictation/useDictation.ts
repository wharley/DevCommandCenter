import { ExpoSpeechRecognitionModule, useSpeechRecognitionEvent } from "expo-speech-recognition";
import { useEffect, useMemo, useRef, useState } from "react";
import { Platform } from "react-native";
import { DictationController, type DictationState, type SpeechModule } from "./controller";

/**
 * Dictation for one text field. Speech events are global, so every mounted
 * composer listens; only the one that started a session acts on them.
 */
export function useDictation(text: string, setText: (text: string) => void) {
	const textRef = useRef(text);
	textRef.current = text;
	const setTextRef = useRef(setText);
	setTextRef.current = setText;
	const [state, setState] = useState<DictationState>({
		phase: "idle",
		error: null,
		needsSettings: false,
		onDevice: false,
	});
	const controller = useMemo(
		() =>
			new DictationController({
				module: ExpoSpeechRecognitionModule as unknown as SpeechModule,
				platform: Platform.OS === "ios" ? "ios" : Platform.OS === "android" ? "android" : "web",
				getText: () => textRef.current,
				setText: (next) => setTextRef.current(next),
				onState: setState,
			}),
		[],
	);
	useEffect(() => () => controller.dispose(), [controller]);

	useSpeechRecognitionEvent("start", () => controller.handleStart());
	useSpeechRecognitionEvent("result", (event) =>
		controller.handleResult(event.results[0]?.transcript ?? "", event.isFinal),
	);
	useSpeechRecognitionEvent("error", (event) => controller.handleError(event.error));
	useSpeechRecognitionEvent("end", () => controller.handleEnd());

	const available = useMemo(() => controller.available(), [controller]);
	return {
		state,
		available,
		listening: state.phase !== "idle",
		toggle: () => void controller.toggle(),
		stop: () => controller.stop(),
		clearError: () => setState((current) => ({ ...current, error: null, needsSettings: false })),
	};
}

export type Dictation = ReturnType<typeof useDictation>;
