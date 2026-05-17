declare module "qrcode" {
	type QrCodeToStringOptions = {
		type?: "svg" | "utf8" | "terminal";
		errorCorrectionLevel?: "L" | "M" | "Q" | "H";
		margin?: number;
		width?: number;
		color?: {
			dark?: string;
			light?: string;
		};
	};

	const QRCode: {
		toString(value: string, options?: QrCodeToStringOptions): Promise<string>;
	};

	export default QRCode;
}
