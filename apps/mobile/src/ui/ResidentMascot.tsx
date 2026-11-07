/**
 * A resident agent's octopus, the desktop's own drawing
 * (`residentMascotSvg`): asleep while nothing wakes it, watching once a
 * trigger is on, working and waiting on you like on the Mac. It bobs while
 * the agent works, unless the system asks for less motion.
 */
import { residentMascotSvg, type ResidentAgentAvatar, type ResidentAgentState } from "@dcc/mobile-core";
import { useEffect, useMemo } from "react";
import Animated, {
	cancelAnimation,
	useAnimatedStyle,
	useReducedMotion,
	useSharedValue,
	withRepeat,
	withTiming,
} from "react-native-reanimated";
import { SvgXml } from "react-native-svg";

/** The desktop's `dcc-agent-bob`: 1.1 s down and up. */
const BOB_MS = 550;

export function ResidentMascot({
	avatar,
	state,
	size = 40,
}: {
	avatar: ResidentAgentAvatar;
	state: ResidentAgentState;
	size?: number;
}) {
	const xml = useMemo(() => residentMascotSvg(avatar, state), [avatar, state]);
	const reduced = useReducedMotion();
	const offset = useSharedValue(0);
	const bobbing = state === "working" && !reduced;
	useEffect(() => {
		if (!bobbing) {
			cancelAnimation(offset);
			offset.value = 0;
			return;
		}
		offset.value = withRepeat(withTiming(-size * 0.08, { duration: BOB_MS }), -1, true);
		return () => cancelAnimation(offset);
	}, [bobbing, offset, size]);
	const style = useAnimatedStyle(() => ({ transform: [{ translateY: offset.value }] }));
	return (
		<Animated.View style={[{ width: size, height: size }, style]}>
			<SvgXml xml={xml} width={size} height={size} />
		</Animated.View>
	);
}
