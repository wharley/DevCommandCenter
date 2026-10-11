/**
 * Dictation into the composer (the phone's native recognizer): what the field
 * shows while the person speaks. Whatever was typed before stays; the speech
 * goes after it. Final results are finished utterances (Android's continuous
 * mode starts a new one after each pause); a partial result replaces the
 * previous partial of the same utterance. Nothing here sends anything.
 */

export type DictationDraft = {
	/** The field's text when dictation started. */
	base: string;
	/** Utterances the recognizer already finalized. */
	committed: string;
	/** The utterance still being recognized. */
	partial: string;
};

export function startDictation(base: string): DictationDraft {
	return { base, committed: "", partial: "" };
}

export function dictationResult(draft: DictationDraft, transcript: string, isFinal: boolean): DictationDraft {
	const spoken = transcript.trim();
	if (isFinal) return { ...draft, committed: joinSpoken(draft.committed, spoken), partial: "" };
	return { ...draft, partial: spoken };
}

/** The field's text: what was there, a space if needed, then the speech. */
export function dictationText(draft: DictationDraft): string {
	const spoken = joinSpoken(draft.committed, draft.partial);
	if (!spoken) return draft.base;
	if (!draft.base || /\s$/.test(draft.base)) return draft.base + spoken;
	return `${draft.base} ${spoken}`;
}

function joinSpoken(left: string, right: string): string {
	if (!left) return right;
	if (!right) return left;
	return `${left} ${right}`;
}

export const DICTATION_LOCALE = "pt-BR";

/** Whether the recognizer can run `locale` without the network. */
export function hasOfflineLocale(installed: readonly string[], locale = DICTATION_LOCALE): boolean {
	const wanted = normalizeLocale(locale);
	return installed.some((candidate) => normalizeLocale(candidate) === wanted);
}

function normalizeLocale(locale: string): string {
	return locale.replace(/_/g, "-").toLowerCase();
}

/**
 * What to tell the person when the recognizer stops with an error, in
 * Portuguese. `null`: nothing to say (they stopped it, or just didn't speak).
 */
export function dictationErrorMessage(code: string): string | null {
	switch (code) {
		case "aborted":
		case "no-speech":
		case "speech-timeout":
			return null;
		case "not-allowed":
			return "Sem permissão para usar o microfone ou o reconhecimento de voz. Libere nos Ajustes do aparelho.";
		case "service-not-allowed":
			return "O reconhecimento de voz está desligado neste aparelho.";
		case "language-not-supported":
			return "Este aparelho não reconhece fala em português.";
		case "network":
			return "O reconhecimento de voz precisa de internet neste aparelho e a conexão falhou.";
		case "audio-capture":
			return "Não foi possível usar o microfone.";
		case "interrupted":
			return "O ditado foi interrompido (ligação, Siri ou alarme).";
		case "busy":
			return "O reconhecimento de voz está ocupado. Tente de novo em instantes.";
		default:
			return "O ditado parou com um erro.";
	}
}
