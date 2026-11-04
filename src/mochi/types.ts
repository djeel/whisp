// Mochi shared types (port fidèle de la garde-robe coucou).
export type MochiMood =
  | "idle"
  | "thinking"
  | "working"
  | "finished"
  | "error"
  | "approval";

export type OutfitId = "none" | "beanie" | "santaHat" | "crown" | "sunglasses" | "scarf";

export function moodFromState(state: string, pendingApproval: boolean): MochiMood {
  if (pendingApproval) return "approval";
  switch (state) {
    case "working":
      return "working";
    case "thinking":
      return "thinking";
    case "finished":
      return "finished";
    case "error":
      return "error";
    default:
      return "idle";
  }
}

const OUTFIT_KEY = "whisp:mochi-outfit";

const VALID_OUTFITS: readonly OutfitId[] = [
  "none",
  "beanie",
  "santaHat",
  "crown",
  "sunglasses",
  "scarf",
];

export function readOutfit(): OutfitId {
  try {
    const raw = localStorage.getItem(OUTFIT_KEY);
    if (VALID_OUTFITS.includes(raw as OutfitId)) return raw as OutfitId;
  } catch {
    /* storage unavailable */
  }
  return "none";
}

export function writeOutfit(id: OutfitId): void {
  try {
    localStorage.setItem(OUTFIT_KEY, id);
  } catch {
    /* storage unavailable */
  }
}

/** Migration douce depuis l'ancien système de tenues (default/noir/fun). */
export function migrateOutfit(raw: string | null): OutfitId {
  if (VALID_OUTFITS.includes(raw as OutfitId)) return raw as OutfitId;
  return "none";
}
