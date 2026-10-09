/**
 * Hermes has no Web Crypto. @noble (Noise keys, nonces) only needs
 * `crypto.getRandomValues`, backed here by the OS CSPRNG through expo-crypto —
 * the same choice as Paseo (`packages/app/src/polyfills/crypto.ts`).
 * TextDecoder and URLSearchParams come from Expo's runtime (`expo/src/winter`).
 */
import { getRandomValues } from "expo-crypto";

type CryptoLike = { getRandomValues?: <T extends ArrayBufferView | null>(array: T) => T };
const target = globalThis as unknown as { crypto?: CryptoLike };

if (typeof target.crypto?.getRandomValues !== "function") {
	const random = <T extends ArrayBufferView | null>(array: T): T => {
		if (!array) return array;
		// expo-crypto fills integer typed arrays; view any buffer as bytes.
		getRandomValues(new Uint8Array(array.buffer, array.byteOffset, array.byteLength));
		return array;
	};
	if (target.crypto) target.crypto.getRandomValues = random;
	else target.crypto = { getRandomValues: random };
}
