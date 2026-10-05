/** Side of the square PNG a project logo is stored as. */
export const PROJECT_LOGO_SIZE = 64;
/** Larger sources are refused before decoding. */
export const MAX_PROJECT_LOGO_SOURCE_BYTES = 2 * 1024 * 1024;

export const PROJECT_LOGO_ACCEPT = "image/png,image/jpeg,image/webp,image/svg+xml";

/** Center-crops the largest square of a `width`×`height` image. */
export function squareCrop(width: number, height: number) {
	const side = Math.min(width, height);
	return { x: (width - side) / 2, y: (height - side) / 2, side };
}

/**
 * Turns an uploaded image into the small PNG data URI DCC stores for a
 * project logo: center-cropped to a square and scaled to 64px, so it stays
 * a few KB in every project listing (desktop and mobile).
 */
export async function projectLogoFromFile(file: File): Promise<string> {
	if (!PROJECT_LOGO_ACCEPT.split(",").includes(file.type)) {
		throw new Error("unsupported image type");
	}
	if (file.size > MAX_PROJECT_LOGO_SOURCE_BYTES) {
		throw new Error("image is too large");
	}
	const url = URL.createObjectURL(file);
	try {
		const image = await new Promise<HTMLImageElement>((resolve, reject) => {
			const element = new Image();
			element.onload = () => resolve(element);
			element.onerror = () => reject(new Error("image could not be decoded"));
			element.src = url;
		});
		const width = image.naturalWidth || PROJECT_LOGO_SIZE;
		const height = image.naturalHeight || PROJECT_LOGO_SIZE;
		const crop = squareCrop(width, height);
		const canvas = document.createElement("canvas");
		canvas.width = PROJECT_LOGO_SIZE;
		canvas.height = PROJECT_LOGO_SIZE;
		const context = canvas.getContext("2d");
		if (!context) throw new Error("canvas unavailable");
		context.imageSmoothingQuality = "high";
		context.drawImage(
			image,
			crop.x,
			crop.y,
			crop.side,
			crop.side,
			0,
			0,
			PROJECT_LOGO_SIZE,
			PROJECT_LOGO_SIZE,
		);
		return canvas.toDataURL("image/png");
	} finally {
		URL.revokeObjectURL(url);
	}
}
