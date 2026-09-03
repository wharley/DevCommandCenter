const SEARCH_URL = "https://www.google.com/search?q=";

const EXPLICIT_SCHEME = /^[a-z][a-z\d+.-]*:/i;
const LOOPBACK_ADDRESS = /^(?:localhost|127(?:\.\d{1,3}){3}|\[::1\])(?::\d+)?(?:[/?#]|$)/i;
const IPV4_ADDRESS = /^(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?(?:[/?#]|$)/;
const DOMAIN_ADDRESS = /^(?:[a-z\d](?:[a-z\d-]{0,61}[a-z\d])?\.)+[a-z]{2,63}(?::\d+)?(?:[/?#]|$)/i;

function looksLikeAddress(value: string): boolean {
	return IPV4_ADDRESS.test(value) || DOMAIN_ADDRESS.test(value);
}

/**
 * Gives the human-facing address bar familiar omnibox behavior while leaving
 * the native command boundary strict. Agent navigation never passes through
 * this resolver and must continue to provide an explicit, validated URL.
 */
export function resolveHumanBrowserAddress(raw: string): string {
	const value = raw.trim();
	if (!value) return "";

	if (LOOPBACK_ADDRESS.test(value)) return `http://${value}`;
	if (value.startsWith("//")) return `https:${value}`;
	if (EXPLICIT_SCHEME.test(value)) return value;
	if (!/[\s\\]/.test(value) && looksLikeAddress(value)) return `https://${value}`;

	// Do not send filesystem-looking input to an external search provider.
	// The strict backend will reject it with the normal URL error instead.
	if (/^(?:\.{1,2}[\\/]|[~/\\])/.test(value)) return value;

	return `${SEARCH_URL}${encodeURIComponent(value)}`;
}
