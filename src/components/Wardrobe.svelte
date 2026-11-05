<script lang="ts">
  import "./Wardrobe.css";
    import { drawBody, drawEyes, makeHead, REAL_OUTFITS } from "../mochi/realEngine";
  import type { OutfitId } from "../mochi/types";

  interface Props {
    value: OutfitId;
    onChange: (id: OutfitId) => void;
  }
  let { value, onChange }: Props = $props();

  const LABELS: Record<OutfitId, string> = {
    none: "Nature",
    beanie: "Bonnet",
    santaHat: "Bonnet de Noël",
    crown: "Couronne",
    sunglasses: "Lunettes",
    scarf: "Écharpe",
  };
  const IDS: OutfitId[] = ["none", "beanie", "santaHat", "crown", "sunglasses", "scarf"];

  function drawMini(canvas: HTMLCanvasElement | null, outfit: OutfitId): void {
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const S = 56;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = Math.round(S * dpr);
    canvas.height = Math.round(S * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, S, S);
    const H = makeHead(S * 0.72, { yaw: 0, pitch: 0 });
    const o = outfit === "none" ? undefined : REAL_OUTFITS[outfit];
    ctx.save();
    ctx.translate(S / 2, S / 2 + H.R * 0.45 - 6);
    if (o?.back) o.back(ctx, H);
    const path = drawBody(ctx, H, o?.bodyColors);
    drawEyes(ctx, H, path);
    if (o?.front) o.front(ctx, H, path);
    ctx.restore();
  }
</script>

<!--
  Garde-robe Mochi — grille visuelle.
  Chaque tenue est prévisualisée par un mini vrai-renderer statique.
-->
<div class="wardrobe-grid" role="radiogroup" aria-label="Garde-robe Mochi">
  {#each IDS as id (id)}
    <button
      type="button"
      role="radio"
      aria-checked={value === id}
      title={LABELS[id]}
      class="wardrobe-cell{value === id ? ' active' : ''}"
      onclick={() => onChange(id)}
    >
      <canvas
        width={56}
        height={56}
        style:width="56px"
        style:height="56px"
        aria-hidden="true"
        {@attach (canvas) => {
          drawMini(canvas, id);
          return () => {};
        }}
      ></canvas>
      <span>{LABELS[id]}</span>
    </button>
  {/each}
</div>
