import { describe, expect, it } from "vitest";
import { audioPeaks, formatAudioTime, isAudioFilePath } from "./audio-attachment";

describe("audio attachments", () => {
	it("recognizes voice notes and common audio formats", () => {
		expect(isAudioFilePath("/Users/me/Downloads/WhatsApp Audio 2026-10-19 at 16.39.25.opus")).toBe(true);
		expect(isAudioFilePath("/tmp/gravacao.M4A")).toBe(true);
		expect(isAudioFilePath("/tmp/clip.mp4")).toBe(false);
		expect(isAudioFilePath("/tmp/notes.md")).toBe(false);
	});

	it("formats durations like a player", () => {
		expect(formatAudioTime(0)).toBe("0:00");
		expect(formatAudioTime(23.97)).toBe("0:23");
		expect(formatAudioTime(725)).toBe("12:05");
		expect(formatAudioTime(3729)).toBe("1:02:09");
		expect(formatAudioTime(Number.NaN)).toBe("0:00");
	});

	it("scales the loudest slice to 1 and keeps silence at 0", () => {
		const channel = new Float32Array([0, 0, 0.5, -0.5, 0.25, -0.25, 0, 0]);
		expect(audioPeaks(channel, 4)).toEqual([0, 1, 0.5, 0]);
		expect(audioPeaks(new Float32Array(8), 4)).toEqual([0, 0, 0, 0]);
	});
});
