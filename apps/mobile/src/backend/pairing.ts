/**
 * Pairing from a scanned QR, a pasted link or a `dcc://pair` deep link:
 * Noise IKpsk2 with the desktop, then wait for the person to approve this
 * phone there. Both screens show the same fingerprints so they can compare.
 */
import {
	fingerprint,
	parsePairingOffer,
	RemoteClient,
	type PairingOffer,
} from "@dcc/remote-protocol";
import { deviceName, devicePlatform, loadDeviceKey, savePairing, type Pairing } from "./identity";

export type PairingProgress =
	| { step: "connecting"; desktopFingerprint: string }
	| { step: "pending"; desktopFingerprint: string; deviceFingerprint: string };

export function readOffer(link: string): PairingOffer {
	const trimmed = link.trim();
	try {
		return parsePairingOffer(trimmed);
	} catch (error) {
		throw new Error(
			trimmed.startsWith("dcc://pair")
				? `Link de pareamento inválido (${(error as Error).message}).`
				: "Isso não é um link de pareamento do DCC (dcc://pair#…).",
		);
	}
}

export async function pairWithDesktop(
	offer: PairingOffer,
	onProgress: (progress: PairingProgress) => void,
): Promise<{ pairing: Pairing; client: RemoteClient }> {
	const keyPair = await loadDeviceKey();
	const desktopFingerprint = fingerprint(offer.desktopPublicKey);
	onProgress({ step: "connecting", desktopFingerprint });
	const client = await RemoteClient.pair(
		offer,
		{ keyPair, name: deviceName(), platform: devicePlatform() },
		{
			onPending: ({ fingerprint: deviceFingerprint }) =>
				onProgress({ step: "pending", desktopFingerprint, deviceFingerprint }),
		},
	);
	const deviceId = client.deviceId;
	if (!deviceId) {
		client.close();
		throw new Error("O computador aprovou, mas não mandou o id do aparelho.");
	}
	const pairing = await savePairing(offer, deviceId);
	return { pairing, client };
}

/** Plain-language versions of the channel errors a person can act on. */
export function pairingErrorMessage(error: unknown): string {
	const code = (error as { code?: number | null }).code ?? null;
	switch (code) {
		case 4403:
			return "O pareamento foi recusado no computador.";
		case 4410:
			return "Este QR expirou ou já foi usado. Gere outro no computador (Configurações → Dispositivos).";
		case 4429:
			return "O computador já tem o máximo de aparelhos pareados. Remova um em Configurações → Dispositivos.";
		case 4603:
			return "O computador não está conectado ao relay. Confira se o DCC está aberto nele.";
		default:
			break;
	}
	const message = error instanceof Error ? error.message : String(error);
	if (/timed out waiting for approval/.test(message)) {
		return "Ninguém aprovou no computador a tempo. Gere outro QR e tente de novo.";
	}
	if (/timed out connecting/.test(message)) {
		return "Não deu para alcançar o computador pelo relay. Confira a internet dos dois lados.";
	}
	return message;
}
