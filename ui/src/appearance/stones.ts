// 立体棋子 (AppearanceSettings.enhancedStones), this product's own: how the main board and the
// 主题外观 preview draw a stone's shadow and choose among its pictures when it is on.

/** Which of a stone's `count` pictures point (x, y) shows: always the same one for that point, so
 *  repaints and moves elsewhere never swap it. */
export function stonePicture(count: number, x: number, y: number): number {
  let h = Math.imul(((y & 0xffff) << 16) ^ (x & 0xffff) ^ 0x2545f491, 0x9e3779b1);
  h ^= h >>> 15;
  h = Math.imul(h, 0x85ebca6b);
  h ^= h >>> 13;
  return (h >>> 0) % count;
}

/** A stone's shadow under light from the upper left: a wide soft one cast to the lower right and a
 *  dark one where the stone meets the board. `size` is the theme's shadow-size (default 85), which
 *  scales both. Painted before any stone, so no shadow falls over a neighbouring stone. */
export function softStoneShadow(g: CanvasRenderingContext2D, cx: number, cy: number, r: number, size: number): void {
  const k = size / 85;
  const dx = cx + r * 0.16 * k, dy = cy + r * 0.22 * k, far = r * (1 + 0.38 * k);
  let shade = g.createRadialGradient(dx, dy, r * 0.45, dx, dy, far);
  shade.addColorStop(0, "rgba(30,18,5,0.34)");
  shade.addColorStop(0.55, "rgba(30,18,5,0.16)");
  shade.addColorStop(1, "rgba(30,18,5,0)");
  g.fillStyle = shade;
  g.beginPath();
  g.arc(dx, dy, far, 0, Math.PI * 2);
  g.fill();
  const tx = cx + r * 0.05 * k, ty = cy + r * 0.07 * k, edge = r * (1 + 0.07 * k);
  shade = g.createRadialGradient(tx, ty, r * 0.82, tx, ty, edge);
  shade.addColorStop(0, "rgba(10,5,0,0.55)");
  shade.addColorStop(1, "rgba(10,5,0,0)");
  g.fillStyle = shade;
  g.beginPath();
  g.arc(tx, ty, edge, 0, Math.PI * 2);
  g.fill();
}
