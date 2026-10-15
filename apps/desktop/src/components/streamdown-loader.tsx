import { lazy, useMemo } from "react";
import type { StreamdownProps } from "streamdown";
import "streamdown/styles.css";

export const LazyStreamdown = lazy(async () => {
	const [{ Streamdown }, { streamdownComponents }, { unwrapFileLinkTargets }] =
		await Promise.all([
			import("streamdown"),
			import("@/components/streamdown-components"),
			import("@/components/streamdown-link-presentation"),
		]);

	const defaultShikiTheme: NonNullable<StreamdownProps["shikiTheme"]> = [
		"github-light",
		"github-dark",
	];
	const LazyRenderer = (props: StreamdownProps) => {
		const components = useMemo(
			() =>
				props.components
					? { ...streamdownComponents, ...props.components }
					: streamdownComponents,
			[props.components],
		);
		const markdown = useMemo(
			() =>
				typeof props.children === "string"
					? unwrapFileLinkTargets(props.children)
					: props.children,
			[props.children],
		);
		return (
			<Streamdown
				{...props}
				components={components}
				shikiTheme={props.shikiTheme ?? defaultShikiTheme}
			>
				{markdown}
			</Streamdown>
		);
	};

	return {
		default: LazyRenderer,
	};
});
