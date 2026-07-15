import type { SVGProps } from "react";

/** Official Cursor cube mark, kept monochrome so it follows the active theme. */
export function CursorEditorIcon(props: SVGProps<SVGSVGElement>) {
	return (
		<svg
			{...props}
			viewBox="0 0 466.73 532.09"
			fill="currentColor"
			aria-hidden="true"
			focusable="false"
		>
			<path d="M457.43,125.94L244.42,2.96c-6.84-3.95-15.28-3.95-22.12,0L9.3,125.94c-5.75,3.32-9.3,9.46-9.3,16.11v247.99c0,6.65,3.55,12.79,9.3,16.11l213.01,122.98c6.84,3.95,15.28,3.95,22.12,0l213.01-122.98c5.75-3.32,9.3-9.46,9.3-16.11v-247.99c0-6.65-3.55-12.79-9.3-16.11h-.01ZM444.05,151.99l-205.63,356.16c-1.39,2.4-5.06,1.42-5.06-1.36v-233.21c0-4.66-2.49-8.97-6.53-11.31L24.87,145.67c-2.4-1.39-1.42-5.06,1.36-5.06h411.26c5.84,0,9.49,6.33,6.57,11.39h-.01Z" />
		</svg>
	);
}

/** Zed's interlocking Z mark. */
export function ZedEditorIcon(props: SVGProps<SVGSVGElement>) {
	return (
		<svg
			{...props}
			viewBox="0 0 96 96"
			fill="currentColor"
			aria-hidden="true"
			focusable="false"
		>
			<path
				fillRule="evenodd"
				clipRule="evenodd"
				d="M9 6C7.343 6 6 7.343 6 9v66H0V9c0-4.971 4.029-9 9-9h80.379c4.009 0 6.017 4.847 3.182 7.682L43.055 57.188H57V51h6v7.688a4.5 4.5 0 0 1-4.5 4.5H37.055L26.743 73.5H73.5V36h6v37.5a6 6 0 0 1-6 6H20.743l-10.5 10.5H87c1.657 0 3-1.343 3-3V21h6v66c0 4.971-4.029 9-9 9H6.621c-4.009 0-6.017-4.847-3.182-7.682L52.757 39H39v6h-6v-7.5a4.5 4.5 0 0 1 4.5-4.5h21.257l10.5-10.5H22.5V60h-6V22.5a6 6 0 0 1 6-6h52.757L85.757 6H9Z"
			/>
		</svg>
	);
}

function VsCodeMark({
	brandColor,
	...props
}: SVGProps<SVGSVGElement> & { brandColor: string }) {
	return (
		<svg
			{...props}
			viewBox="0 0 16 16"
			fill="none"
			aria-hidden="true"
			focusable="false"
		>
			<path
				fill={brandColor}
				fillRule="evenodd"
				clipRule="evenodd"
				d="M10.509 13.918a.75.75 0 0 0 .596-.023l2.47-1.189A.75.75 0 0 0 14 12.03V3.97a.75.75 0 0 0-.425-.676l-2.47-1.189a.75.75 0 0 0-.763.069 1 1 0 0 0-.09.076L5.523 6.565 3.462 5.001a.5.5 0 0 0-.638.029l-.66.6a.5.5 0 0 0-.001.74L3.95 8l-1.787 1.63a.5.5 0 0 0 .001.74l.66.6a.5.5 0 0 0 .638.029l2.06-1.564 4.73 4.315a.75.75 0 0 0 .257.168Zm.493-8.642L7.413 8l3.589 2.724V5.276Z"
			/>
		</svg>
	);
}

export function VsCodeEditorIcon(props: SVGProps<SVGSVGElement>) {
	return <VsCodeMark {...props} brandColor="#23A8F2" />;
}

export function VsCodeInsidersEditorIcon(props: SVGProps<SVGSVGElement>) {
	return <VsCodeMark {...props} brandColor="#24BFA5" />;
}

/** Compact vector rendition of Trae's official green app/favicon mark. */
export function TraeEditorIcon(props: SVGProps<SVGSVGElement>) {
	return (
		<svg
			{...props}
			viewBox="0 0 48 48"
			fill="none"
			aria-hidden="true"
			focusable="false"
		>
			<path fill="#00E599" d="M8 13h32v19h-4v4H13v-4H8V13Zm5 5v10h23V18H13Z" />
			<path
				fill="#E9FFF8"
				d="m18.5 23 3.5-3.5 3.5 3.5-3.5 3.5-3.5-3.5Zm10 0 3.5-3.5 3.5 3.5-3.5 3.5-3.5-3.5Z"
			/>
		</svg>
	);
}
