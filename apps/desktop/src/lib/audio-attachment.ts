import { invoke } from "@tauri-apps/api/core";
import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { DCC_QUERY_GC_TIME_MS } from "@/lib/query-client";

const AUDIO_FILE_RE = /\.(?:mp3|m4a|mp4a|aac|wav|ogg|oga|opus|flac|aiff?|caf)$/i;

/** True for a path to an audio file the person's message can play inline. */
export function isAudioFilePath(path: string): boolean {
	return AUDIO_FILE_RE.test(path.trim());
}

/** `0:42`, `12:05`, `1:02:09`. */
export function formatAudioTime(seconds: number): string {
	const total = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
	const h = Math.floor(total / 3600);
	const m = Math.floor((total % 3600) / 60);
	const s = String(total % 60).padStart(2, "0");
	return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/**
 * Loudness of `buckets` equal slices of the clip, 0–1, the loudest slice at 1.
 * RMS rather than the peak sample, so a voice note reads as speech and pauses.
 */
export function audioPeaks(channel: Float32Array, buckets: number): number[] {
	const size = Math.max(1, Math.floor(channel.length / buckets));
	const levels: number[] = [];
	for (let bucket = 0; bucket < buckets; bucket++) {
		const start = bucket * size;
		const end = Math.min(channel.length, start + size);
		let sum = 0;
		for (let i = start; i < end; i++) sum += channel[i] * channel[i];
		levels.push(end > start ? Math.sqrt(sum / (end - start)) : 0);
	}
	const loudest = Math.max(...levels, 0);
	return levels.map((level) => (loudest > 0 ? level / loudest : 0));
}

export const AUDIO_WAVEFORM_BARS = 40;

export type AudioAttachmentData = {
	buffer: AudioBuffer;
	duration: number;
	peaks: number[];
};

function isTauriRuntime() {
	return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Decoded through Web Audio rather than played by an `<audio>` element: the
 * desktop WebView decodes Ogg Opus (WhatsApp voice notes) but its media element
 * refuses it.
 */
let decoder: OfflineAudioContext | null = null;
function audioDecoder() {
	decoder ??= new OfflineAudioContext(1, 1, 44_100);
	return decoder;
}

/**
 * An attached audio file, decoded with its waveform. `null` when the file is
 * gone, too large, not decodable or the app runs outside Tauri; callers fall
 * back to the file chip.
 */
export function useAudioAttachment(path: string) {
	return useQuery({
		queryKey: ["audioAttachment", path],
		queryFn: async (): Promise<AudioAttachmentData | null> => {
			if (!isTauriRuntime() || typeof OfflineAudioContext === "undefined") return null;
			try {
				const bytes = await invoke<ArrayBuffer>("read_audio_preview", { path });
				const buffer = await audioDecoder().decodeAudioData(bytes);
				return {
					buffer,
					duration: buffer.duration,
					peaks: audioPeaks(buffer.getChannelData(0), AUDIO_WAVEFORM_BARS),
				};
			} catch {
				return null;
			}
		},
		staleTime: Number.POSITIVE_INFINITY,
		gcTime: DCC_QUERY_GC_TIME_MS.heavyPayload,
		retry: false,
	});
}

let playback: AudioContext | null = null;
/** Only one clip plays at a time across the timeline. */
let stopActiveClip: (() => void) | null = null;

/** Play, pause and seek one decoded clip; `position` follows playback. */
export function useAudioClipPlayer(buffer: AudioBuffer | null) {
	const [playing, setPlaying] = useState(false);
	const [position, setPosition] = useState(0);
	const sourceRef = useRef<AudioBufferSourceNode | null>(null);
	const startedAtRef = useRef(0);
	const offsetRef = useRef(0);
	const frameRef = useRef(0);

	const halt = useCallback(() => {
		cancelAnimationFrame(frameRef.current);
		const source = sourceRef.current;
		sourceRef.current = null;
		if (!source) return;
		source.onended = null;
		try {
			source.stop();
		} catch {
			// Already stopped.
		}
	}, []);

	const pause = useCallback(() => {
		if (!sourceRef.current || !playback) return;
		offsetRef.current = Math.min(
			buffer?.duration ?? 0,
			playback.currentTime - startedAtRef.current,
		);
		halt();
		setPosition(offsetRef.current);
		setPlaying(false);
		if (stopActiveClip === pause) stopActiveClip = null;
	}, [buffer, halt]);

	const start = useCallback(
		(from: number) => {
			if (!buffer) return;
			if (stopActiveClip && stopActiveClip !== pause) stopActiveClip();
			playback ??= new AudioContext();
			const context = playback;
			if (context.state === "suspended") void context.resume();
			halt();
			const offset = from >= buffer.duration ? 0 : Math.max(0, from);
			const source = context.createBufferSource();
			source.buffer = buffer;
			source.connect(context.destination);
			source.onended = () => {
				if (sourceRef.current !== source) return;
				sourceRef.current = null;
				cancelAnimationFrame(frameRef.current);
				offsetRef.current = 0;
				setPosition(0);
				setPlaying(false);
				if (stopActiveClip === pause) stopActiveClip = null;
			};
			startedAtRef.current = context.currentTime - offset;
			offsetRef.current = offset;
			source.start(0, offset);
			sourceRef.current = source;
			stopActiveClip = pause;
			setPosition(offset);
			setPlaying(true);
			const tick = () => {
				setPosition(Math.min(buffer.duration, context.currentTime - startedAtRef.current));
				frameRef.current = requestAnimationFrame(tick);
			};
			frameRef.current = requestAnimationFrame(tick);
		},
		[buffer, halt, pause],
	);

	const toggle = useCallback(() => {
		if (sourceRef.current) pause();
		else start(offsetRef.current);
	}, [pause, start]);

	const seek = useCallback(
		(seconds: number) => {
			const target = Math.min(buffer?.duration ?? 0, Math.max(0, seconds));
			if (sourceRef.current) {
				start(target);
			} else {
				offsetRef.current = target;
				setPosition(target);
			}
		},
		[buffer, start],
	);

	useEffect(
		() => () => {
			halt();
			if (stopActiveClip === pause) stopActiveClip = null;
		},
		[halt, pause],
	);

	return { playing, position, toggle, seek };
}
