import type { SkillContextDetection } from "@/lib/skills-api";

export function getTotalSkillContextCount(detections: SkillContextDetection[]) {
	const dccSourceCount = detections
		.filter((item) => item.kind === "dcc_source")
		.reduce((total, item) => total + item.count, 0);
	const externalCount = detections
		.filter((item) => item.kind !== "dcc_source")
		.reduce((total, item) => total + item.externalCount, 0);

	return dccSourceCount + externalCount;
}
