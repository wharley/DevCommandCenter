import { useEffect, useState } from "react";

export function useSplashScreen() {
	const [isVisible, setIsVisible] = useState(true);
	const [isHiding, setIsHiding] = useState(false);

	useEffect(() => {
		const hideTimer = window.setTimeout(() => {
			setIsHiding(true);
		}, 650);

		const doneTimer = window.setTimeout(() => {
			setIsVisible(false);
		}, 1150);

		return () => {
			window.clearTimeout(hideTimer);
			window.clearTimeout(doneTimer);
		};
	}, []);

	return {
		isHiding,
		isVisible,
	};
}
