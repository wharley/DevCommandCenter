import { useRef, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
import { Pause, Play } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
	AUDIO_WAVEFORM_BARS,
	formatAudioTime,
	useAudioAttachment,
	useAudioClipPlayer,
	type AudioAttachmentData,
} from "@/lib/audio-attachment";
import { pathBasename } from "@/lib/path-basename";
import { cn } from "@/lib/utils";

function splitName(path: string) {
	const name = pathBasename(path);
	const dot = name.lastIndexOf(".");
	return dot > 0
		? { stem: name.slice(0, dot), extension: name.slice(dot + 1).toUpperCase() }
		: { stem: name, extension: null };
}

function AudioCard({
	path,
	button,
	waveform,
	time,
}: {
	path: string;
	button: ReactNode;
	waveform: ReactNode;
	time: string | null;
}) {
	const { stem, extension } = splitName(path);
	return (
		<span
			title={path}
			data-testid="user-message-audio"
			className="inline-flex w-[19rem] max-w-full items-center gap-2.5 rounded-xl border border-border/60 bg-background/70 py-2 pr-3 pl-2"
		>
			{button}
			<span className="flex min-w-0 flex-1 flex-col gap-1">
				{waveform}
				<span className="flex min-w-0 items-baseline gap-2 text-[11px] leading-4 text-muted-foreground">
					<span className="min-w-0 flex-1 truncate">
						{stem}
						{extension ? <span className="opacity-60"> · {extension}</span> : null}
					</span>
					{time ? <span className="shrink-0 tabular-nums">{time}</span> : null}
				</span>
			</span>
		</span>
	);
}

function Bars({ levels, progress }: { levels: number[]; progress: number }) {
	return (
		<span className="flex h-6 items-center justify-between" aria-hidden>
			{levels.map((level, index) => (
				<span
					key={index}
					className={cn(
						"w-[2px] rounded-full transition-colors duration-150",
						(index + 0.5) / levels.length <= progress
							? "bg-foreground/80"
							: "bg-muted-foreground/35",
					)}
					// A gentle curve so speech and pauses read apart, not as a flat band.
					style={{ height: `${Math.max(12, Math.round(level ** 1.5 * 100))}%` }}
				/>
			))}
		</span>
	);
}

function PlayableAudio({ path, data }: { path: string; data: AudioAttachmentData }) {
	const { t } = useTranslation("common");
	const { playing, position, toggle, seek } = useAudioClipPlayer(data.buffer);
	const trackRef = useRef<HTMLSpanElement>(null);
	const name = pathBasename(path);
	const progress = data.duration > 0 ? position / data.duration : 0;

	const seekToPointer = (event: PointerEvent<HTMLSpanElement>) => {
		const rect = trackRef.current?.getBoundingClientRect();
		if (!rect || rect.width === 0) return;
		const ratio = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
		seek(ratio * data.duration);
	};
	const seekWithKeys = (event: KeyboardEvent<HTMLSpanElement>) => {
		const step = event.shiftKey ? 15 : 5;
		if (event.key === "ArrowRight") seek(position + step);
		else if (event.key === "ArrowLeft") seek(position - step);
		else if (event.key === "Home") seek(0);
		else if (event.key === " " || event.key === "Enter") toggle();
		else return;
		event.preventDefault();
	};

	return (
		<AudioCard
			path={path}
			// Like a voice note: the length at rest, the time played once started.
			time={formatAudioTime(position > 0 ? position : data.duration)}
			button={
				<button
					type="button"
					aria-label={t(playing ? "conversation.attachment.pauseAudio" : "conversation.attachment.playAudio", {
						name,
					})}
					aria-pressed={playing}
					className="inline-flex size-8 shrink-0 items-center justify-center rounded-full bg-foreground text-background transition-opacity hover:opacity-85 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
					onClick={toggle}
				>
					{playing ? (
						<Pause className="size-3.5 fill-current" strokeWidth={0} aria-hidden />
					) : (
						<Play className="ml-0.5 size-3.5 fill-current" strokeWidth={0} aria-hidden />
					)}
				</button>
			}
			waveform={
				<span
					ref={trackRef}
					role="slider"
					tabIndex={0}
					aria-label={t("conversation.attachment.audioPosition", { name })}
					aria-valuemin={0}
					aria-valuemax={Math.round(data.duration)}
					aria-valuenow={Math.round(position)}
					aria-valuetext={formatAudioTime(position)}
					className="block cursor-pointer rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
					onPointerDown={(event) => {
						event.currentTarget.setPointerCapture(event.pointerId);
						seekToPointer(event);
					}}
					onPointerMove={(event) => {
						if (event.currentTarget.hasPointerCapture(event.pointerId)) seekToPointer(event);
					}}
					onKeyDown={seekWithKeys}
				>
					<Bars levels={data.peaks} progress={progress} />
				</span>
			}
		/>
	);
}

const LOADING_LEVELS = Array.from({ length: AUDIO_WAVEFORM_BARS }, () => 0.2);

/**
 * An attached audio file plays in place, like a voice note: play/pause, its
 * waveform (click or drag to seek) and duration. Falls back to `fallback` (the
 * file chip) when the file is gone or cannot be decoded.
 */
export function AudioAttachment({ path, fallback }: { path: string; fallback: ReactNode }) {
	const audio = useAudioAttachment(path);
	if (audio.isPending) {
		return (
			<AudioCard
				path={path}
				time={null}
				button={<span className="size-8 shrink-0 animate-pulse rounded-full bg-muted" aria-hidden />}
				waveform={
					<span className="block animate-pulse">
						<Bars levels={LOADING_LEVELS} progress={0} />
					</span>
				}
			/>
		);
	}
	if (!audio.data) return fallback;
	return <PlayableAudio path={path} data={audio.data} />;
}
