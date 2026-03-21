export type ProductSignal =
  | "review_extra_repo_added"
  | "mirror_mission_created";

type ProductSignalsSnapshot = {
  reviewExtraRepoAdds: number;
  mirrorMissionsCreated: number;
  updatedAt: number;
};

const STORAGE_KEY = "dcc:product:signals";

function safeParse(raw: string | null): ProductSignalsSnapshot | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as Partial<ProductSignalsSnapshot>;
    return {
      reviewExtraRepoAdds: Number(parsed.reviewExtraRepoAdds ?? 0),
      mirrorMissionsCreated: Number(parsed.mirrorMissionsCreated ?? 0),
      updatedAt: Number(parsed.updatedAt ?? Date.now()),
    };
  } catch {
    return null;
  }
}

export function readProductSignals(): ProductSignalsSnapshot {
  if (typeof window === "undefined") {
    return {
      reviewExtraRepoAdds: 0,
      mirrorMissionsCreated: 0,
      updatedAt: Date.now(),
    };
  }
  const parsed = safeParse(window.localStorage.getItem(STORAGE_KEY));
  if (parsed) return parsed;
  return {
    reviewExtraRepoAdds: 0,
    mirrorMissionsCreated: 0,
    updatedAt: Date.now(),
  };
}

export function recordProductSignal(signal: ProductSignal): ProductSignalsSnapshot {
  const current = readProductSignals();
  const next: ProductSignalsSnapshot = {
    ...current,
    updatedAt: Date.now(),
  };

  if (signal === "review_extra_repo_added") {
    next.reviewExtraRepoAdds += 1;
  } else if (signal === "mirror_mission_created") {
    next.mirrorMissionsCreated += 1;
  }

  if (typeof window !== "undefined") {
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    } catch {
      // best effort only
    }
  }

  return next;
}

/**
 * We intentionally avoid adding a new DB field now.
 * This hint helps validate if storyRef deserves schema/API complexity.
 */
export function shouldSuggestStoryRef(snapshot?: ProductSignalsSnapshot): boolean {
  const s = snapshot ?? readProductSignals();
  return s.reviewExtraRepoAdds + s.mirrorMissionsCreated >= 3;
}
