import { useEffect, useState } from "react";
import { Globe } from "lucide-react";
import { loadSiteFavicon } from "@/lib/site-favicon";

export function SiteLinkIcon({
	url,
	className,
}: {
	url: string;
	className?: string;
}) {
	const [loaded, setLoaded] = useState<{
		url: string;
		src: string | null;
	} | null>(null);
	useEffect(() => {
		let active = true;
		void loadSiteFavicon(url).then((src) => {
			if (active) setLoaded({ url, src });
		});
		return () => {
			active = false;
		};
	}, [url]);
	return loaded?.url === url && loaded.src ? (
		<img
			src={loaded.src}
			alt=""
			aria-hidden
			draggable={false}
			className={className}
			referrerPolicy="no-referrer"
			onError={() => setLoaded({ url, src: null })}
		/>
	) : (
		<Globe aria-hidden className={className} />
	);
}
