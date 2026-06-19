import { FitAddon } from "@xterm/addon-fit";
import { type ILinkProvider, type ITheme, Terminal } from "xterm";
import { memo, useEffect, useRef, type MutableRefObject } from "react";
import "xterm/css/xterm.css";

type TerminalOutputProps = {
	terminalRef?: MutableRefObject<TerminalHandle | null>;
	className?: string;
	detectLinks?: boolean;
	fontSize?: number;
	lineHeight?: number;
	padding?: string;
	onData?: (data: string) => void;
	onResize?: (cols: number, rows: number) => void;
};

export type TerminalHandle = {
	write: (data: string) => void;
	clear: () => void;
	dispose: () => void;
	refit: () => void;
	focus: () => void;
};

const URL_PATTERN = /https?:\/\/[^\s<>"'`]+/gi;
const TRAILING_URL_PUNCTUATION = /[),.;:!?]+$/;

function sanitizeHttpUrl(value: string): string | null {
	const trimmed = value.replace(TRAILING_URL_PUNCTUATION, "");
	try {
		const url = new URL(trimmed);
		if (url.protocol !== "http:" && url.protocol !== "https:") return null;
		return url.toString();
	} catch {
		return null;
	}
}

function openHttpUrl(value: string) {
	const url = sanitizeHttpUrl(value);
	if (!url) return;
	window.open(url, "_blank", "noopener,noreferrer");
}

function createHttpLinkProvider(terminal: Terminal): ILinkProvider {
	return {
		provideLinks(bufferLineNumber, callback) {
			const buffer = terminal.buffer.active;
			const line = buffer.getLine(bufferLineNumber - 1);
			const text = line?.translateToString(false) ?? "";
			const links = [...text.matchAll(URL_PATTERN)]
				.map((match) => {
					if (match.index === undefined) return null;
					const rawText = match[0];
					const url = sanitizeHttpUrl(rawText);
					if (!url) return null;
					return {
						range: {
							start: { x: match.index + 1, y: bufferLineNumber },
							end: {
								x: match.index + rawText.replace(TRAILING_URL_PUNCTUATION, "").length + 1,
								y: bufferLineNumber,
							},
						},
						text: url,
						decorations: {
							pointerCursor: true,
							underline: true,
						},
						activate: (_event: MouseEvent, linkText: string) => {
							openHttpUrl(linkText);
						},
					};
				})
				.filter((link) => link !== null);

			callback(links.length > 0 ? links : undefined);
		},
	};
}

let terminalFitSuspendCount = 0;
const terminalRefitListeners = new Set<() => void>();

export function suspendTerminalFit(): () => void {
	terminalFitSuspendCount++;
	let released = false;
	return () => {
		if (released) return;
		released = true;
		terminalFitSuspendCount--;
		if (terminalFitSuspendCount === 0) {
			for (const listener of terminalRefitListeners) {
				listener();
			}
		}
	};
}

function resolveTerminalTheme(): ITheme {
	const s = getComputedStyle(document.documentElement);
	const v = (name: string) => s.getPropertyValue(`--terminal-${name}`).trim();
	return {
		background: v("background"),
		foreground: v("foreground"),
		cursor: v("cursor"),
		selectionBackground: v("selection"),
		black: v("black"),
		red: v("red"),
		green: v("green"),
		yellow: v("yellow"),
		blue: v("blue"),
		magenta: v("magenta"),
		cyan: v("cyan"),
		white: v("white"),
		brightBlack: v("bright-black"),
		brightRed: v("bright-red"),
		brightGreen: v("bright-green"),
		brightYellow: v("bright-yellow"),
		brightBlue: v("bright-blue"),
		brightMagenta: v("bright-magenta"),
		brightCyan: v("bright-cyan"),
		brightWhite: v("bright-white"),
	};
}

function TerminalOutputImpl({
	terminalRef,
	className,
	detectLinks = false,
	fontSize = 12,
	lineHeight = 1.3,
	padding = "12px 2px 12px 12px",
	onData,
	onResize,
}: TerminalOutputProps) {
	const containerRef = useRef<HTMLDivElement>(null);
	const onDataRef = useRef<typeof onData>(onData);
	const onResizeRef = useRef<typeof onResize>(onResize);
	onDataRef.current = onData;
	onResizeRef.current = onResize;

	useEffect(() => {
		const container = containerRef.current;
		if (!container) return;

		const fit = new FitAddon();
		const terminal = new Terminal({
			convertEol: false,
			disableStdin: false,
			scrollback: 5000,
			fontSize,
			fontFamily: "'Geist Mono Variable', 'Geist Mono', monospace",
			lineHeight,
			theme: resolveTerminalTheme(),
			cursorBlink: false,
			cursorStyle: "bar",
			cursorInactiveStyle: "none",
			macOptionIsMeta: true,
		});

		terminal.loadAddon(fit);
		terminal.open(container);
		terminal.element?.style.setProperty("padding", padding);

		let fitFrame: number | null = null;
		let lastFitWidth = -1;
		let lastFitHeight = -1;
		let lastResizeCols = -1;
		let lastResizeRows = -1;

		const fitNow = () => {
			if (terminalFitSuspendCount > 0 || fitFrame !== null) return;
			fitFrame = requestAnimationFrame(() => {
				fitFrame = null;
				const width = container.clientWidth;
				const height = container.clientHeight;
				if (width === 0 || height === 0) return;
				if (width === lastFitWidth && height === lastFitHeight) return;
				lastFitWidth = width;
				lastFitHeight = height;
				try {
					fit.fit();
				} catch {
					// Detached container during resize transition.
				}
			});
		};

		const refitListener = () => {
			fitNow();
		};
		terminalRefitListeners.add(refitListener);

		const linkProviderDisposable = detectLinks
			? terminal.registerLinkProvider(createHttpLinkProvider(terminal))
			: null;

		fitNow();

		const dataSub = terminal.onData((data) => {
			onDataRef.current?.(data);
		});

		const resizeSub = terminal.onResize(({ cols, rows }) => {
			if (cols === lastResizeCols && rows === lastResizeRows) return;
			lastResizeCols = cols;
			lastResizeRows = rows;
			onResizeRef.current?.(cols, rows);
		});

		const resizeObserver = new ResizeObserver(() => {
			if (
				container.clientWidth === 0 ||
				container.clientHeight === 0
			) {
				return;
			}
			fitNow();
		});
		resizeObserver.observe(container);

		const themeObserver = new MutationObserver(() => {
			terminal.options.theme = resolveTerminalTheme();
		});
		themeObserver.observe(document.documentElement, {
			attributes: true,
			attributeFilter: ["class"],
		});

		if (terminalRef) {
			terminalRef.current = {
				write: (data: string) => terminal.write(data),
				clear: () => terminal.clear(),
				dispose: () => terminal.dispose(),
				refit: () => fitNow(),
				focus: () => terminal.focus(),
			};
		}

		return () => {
			if (fitFrame !== null) {
				cancelAnimationFrame(fitFrame);
			}
			dataSub.dispose();
			resizeSub.dispose();
			linkProviderDisposable?.dispose();
			themeObserver.disconnect();
			resizeObserver.disconnect();
			terminalRefitListeners.delete(refitListener);
			terminal.dispose();
			if (terminalRef) {
				terminalRef.current = null;
			}
		};
	}, [detectLinks, fontSize, lineHeight, padding, terminalRef]);

	return <div ref={containerRef} className={className} />;
}

export const TerminalOutput = memo(TerminalOutputImpl);
