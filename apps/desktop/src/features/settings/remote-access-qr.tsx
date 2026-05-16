import { useEffect, useState } from "react";
import QRCode from "qrcode";
import { cn } from "@/lib/utils";

type RemoteAccessQrProps = {
	value: string;
	size?: number;
	className?: string;
};

export function RemoteAccessQr({
	value,
	size = 224,
	className,
}: RemoteAccessQrProps) {
	const [svg, setSvg] = useState<string>("");

	useEffect(() => {
		let cancelled = false;
		void QRCode.toString(value, {
			type: "svg",
			errorCorrectionLevel: "M",
			margin: 1,
			width: size,
			color: {
				dark: "#111827",
				light: "#ffffff",
			},
		})
			.then((nextSvg: string) => {
				if (!cancelled) {
					setSvg(nextSvg);
				}
			})
			.catch(() => {
				if (!cancelled) {
					setSvg("");
				}
			});

		return () => {
			cancelled = true;
		};
	}, [size, value]);

	if (!svg) {
		return (
			<div
				className={cn(
					"flex items-center justify-center rounded-xl border border-dashed border-border/70 bg-background p-4 text-center text-[11px] text-muted-foreground",
					className,
				)}
				style={{ width: size, height: size }}
			>
				QR indisponivel
			</div>
		);
	}

	return (
		<div
			className={cn("overflow-hidden rounded-xl border border-border/70 bg-white p-2", className)}
			dangerouslySetInnerHTML={{ __html: svg }}
		/>
	);
}
