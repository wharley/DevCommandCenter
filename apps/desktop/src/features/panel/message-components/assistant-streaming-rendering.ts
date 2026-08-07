import type { StreamdownProps } from "streamdown";

export const ASSISTANT_STREAMING_ANIMATION_CHARACTER_LIMIT = 2400;

export const ASSISTANT_STREAMDOWN_SHIKI_THEME: NonNullable<
	StreamdownProps["shikiTheme"]
> = ["github-light", "github-dark"];

export const ASSISTANT_STREAMDOWN_WORD_ANIMATION: NonNullable<
	StreamdownProps["animated"]
> = {
	animation: "blurIn",
	duration: 150,
	stagger: 30,
	sep: "word",
};

export function assistantStreamingAnimation(
	streaming: boolean | undefined,
	contentLength: number,
): StreamdownProps["animated"] {
	return streaming && contentLength <= ASSISTANT_STREAMING_ANIMATION_CHARACTER_LIMIT
		? ASSISTANT_STREAMDOWN_WORD_ANIMATION
		: false;
}
