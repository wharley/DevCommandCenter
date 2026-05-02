import { InlineShortcutDisplay } from "./InlineShortcutDisplay";

export function ShortcutDisplay({
	hotkey,
}: {
	hotkey: string;
}) {
	return <InlineShortcutDisplay keys={[hotkey]} />;
}
