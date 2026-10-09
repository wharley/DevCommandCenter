/**
 * `dcc://pair#…` opens the pairing screen. The router would drop the URL
 * fragment (where the one-time secret lives), so the raw link is stashed first.
 */
import { isPairingLink, stashPairingLink } from "@/backend/offerLink";

export function redirectSystemPath({ path }: { path: string; initial: boolean }): string {
	if (isPairingLink(path)) {
		stashPairingLink(path);
		return "/pair";
	}
	return path;
}
