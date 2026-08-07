import { describe, expect, it } from "vitest";
import {
	ASSISTANT_STREAMDOWN_WORD_ANIMATION,
	ASSISTANT_STREAMING_ANIMATION_CHARACTER_LIMIT,
	assistantStreamingAnimation,
} from "./assistant-streaming-rendering";

describe("assistantStreamingAnimation", () => {
	it("keeps one stable word-animation config for small streaming replies", () => {
		expect(assistantStreamingAnimation(true, 200)).toBe(
			ASSISTANT_STREAMDOWN_WORD_ANIMATION,
		);
		expect(
			assistantStreamingAnimation(
				true,
				ASSISTANT_STREAMING_ANIMATION_CHARACTER_LIMIT,
			),
		).toBe(ASSISTANT_STREAMDOWN_WORD_ANIMATION);
	});

	it("disables word animation once a streaming reply becomes large", () => {
		expect(
			assistantStreamingAnimation(
				true,
				ASSISTANT_STREAMING_ANIMATION_CHARACTER_LIMIT + 1,
			),
		).toBe(false);
	});

	it("never animates settled content", () => {
		expect(assistantStreamingAnimation(false, 10)).toBe(false);
	});
});
