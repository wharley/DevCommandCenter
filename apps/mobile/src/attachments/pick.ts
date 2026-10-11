/**
 * Picking what to attach: photos from the library or the camera, any other
 * file from the document picker. Photos are resized (longest edge
 * `IMAGE_MAX_EDGE`) and re-encoded as JPEG before upload — that also turns
 * HEIC into something every agent reads.
 */
import { attachmentRefusal, IMAGE_JPEG_QUALITY, resizedDimensions } from "@dcc/mobile-core";
import * as DocumentPicker from "expo-document-picker";
import { File } from "expo-file-system";
import { ImageManipulator, SaveFormat } from "expo-image-manipulator";
import * as ImagePicker from "expo-image-picker";
import type { PreparedAttachment } from "./queue";

export type PickSource = "library" | "camera" | "document";

export type PickOutcome =
	/** `refused`: files left out, with the reason (too large). */
	| { kind: "picked"; files: PreparedAttachment[]; refused: string[] }
	| { kind: "cancelled" }
	| { kind: "denied"; message: string };

export async function pickAttachments(source: PickSource, room: number): Promise<PickOutcome> {
	if (room <= 0) return { kind: "cancelled" };
	if (source === "document") return pickDocuments();
	if (source === "camera") {
		const permission = await ImagePicker.requestCameraPermissionsAsync();
		if (!permission.granted) {
			return { kind: "denied", message: "Sem permissão para a câmera. Libere nos Ajustes do aparelho." };
		}
	}
	const options: ImagePicker.ImagePickerOptions = {
		mediaTypes: ["images"],
		quality: 1,
		exif: false,
		...(source === "library" ? { allowsMultipleSelection: room > 1, selectionLimit: room } : {}),
	};
	const result =
		source === "camera" ? await ImagePicker.launchCameraAsync(options) : await ImagePicker.launchImageLibraryAsync(options);
	if (result.canceled) return { kind: "cancelled" };
	const files: PreparedAttachment[] = [];
	for (const asset of result.assets.slice(0, room)) files.push(await preparePhoto(asset));
	return { kind: "picked", files, refused: [] };
}

async function preparePhoto(asset: ImagePicker.ImagePickerAsset): Promise<PreparedAttachment> {
	const context = ImageManipulator.manipulate(asset.uri);
	const size = resizedDimensions(asset.width, asset.height);
	if (size) context.resize(size);
	const image = await context.renderAsync();
	const saved = await image.saveAsync({ compress: IMAGE_JPEG_QUALITY, format: SaveFormat.JPEG, base64: true });
	const stem = (asset.fileName ?? "foto").replace(/\.[^.]+$/, "");
	return { kind: "image", name: `${stem}.jpg`, previewUri: saved.uri, base64: saved.base64 ?? "" };
}

async function pickDocuments(): Promise<PickOutcome> {
	const result = await DocumentPicker.getDocumentAsync({ multiple: true, copyToCacheDirectory: true });
	if (result.canceled) return { kind: "cancelled" };
	const files: PreparedAttachment[] = [];
	const refused: string[] = [];
	for (const asset of result.assets) {
		// Refused by the size the picker reports, before reading it into memory.
		const refusal = asset.size ? attachmentRefusal(asset.size, 0) : null;
		if (refusal) {
			refused.push(`${asset.name}: ${refusal}`);
			continue;
		}
		const isImage = asset.mimeType?.startsWith("image/") ?? false;
		files.push({
			kind: isImage ? "image" : "file",
			name: asset.name,
			previewUri: isImage ? asset.uri : null,
			base64: await new File(asset.uri).base64(),
		});
	}
	return { kind: "picked", files, refused };
}
