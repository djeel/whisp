import type { OutfitId } from "./types";

/** Garde-robe complète : ids fidèles à la référence coucou + labels FR. */
export const OUTFIT_OPTIONS: Array<{ id: OutfitId; label: string }> = [
  { id: "none", label: "Nature" },
  { id: "beanie", label: "Bonnet" },
  { id: "santaHat", label: "Bonnet de Noël" },
  { id: "crown", label: "Couronne" },
  { id: "sunglasses", label: "Lunettes" },
  { id: "scarf", label: "Écharpe" },
];
