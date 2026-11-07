// 抓取死活题's full-screen overlay (CaptureTsumeGo1.2.jar CaptureTsumego): the screen picture under
// Java's translucent dimming, then three left clicks: the origin (0,0), the first square's
// intersection (1,1) and the corner that covers the problem. Escape cancels at any step. Rust reads
// the stones; every coordinate sent is a picture pixel.
import { invoke } from "@tauri-apps/api/core";
import type { CapturePreview } from "../contract/generated";
import "./capture.css";

type Pixel = [number, number];

const CROSSHAIR = "rgba(0, 245, 0, 0.784)"; // Color(0, 245, 0, 200)
const GUIDE = "rgba(0, 0, 200, 0.471)"; // Color(0, 0, 200, 120)
const AREA = "rgba(0, 0, 200, 0.784)"; // Color(0, 0, 200, 200)
const DIM = "rgba(0, 0, 0, 0.098)"; // the window background Color(0, 0, 0, 25)

export async function bootCaptureOverlay(): Promise<void> {
  const canvas = document.createElement("canvas");
  document.body.append(canvas);
  const g = canvas.getContext("2d")!;
  document.addEventListener("contextmenu", (e) => e.preventDefault());
  document.addEventListener("dragstart", (e) => e.preventDefault());
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") void invoke("tsumego_capture_cancel");
  });
  window.focus();

  let picture: ImageBitmap;
  try {
    const bytes = await invoke<ArrayBuffer>("tsumego_capture_picture");
    const header = new DataView(bytes, 0, 8);
    const width = header.getUint32(0, true);
    const height = header.getUint32(4, true);
    picture = await createImageBitmap(new ImageData(new Uint8ClampedArray(bytes, 8, width * height * 4), width, height));
  } catch (e) {
    // Without the picture nothing can be marked: give the screen back.
    console.error("capture picture", e);
    await invoke("tsumego_capture_cancel");
    return;
  }
  const { width, height } = picture;
  canvas.width = width;
  canvas.height = height;

  let clickTimes = 0;
  let cursor: Pixel | null = null;
  let origin: Pixel = [0, 0];
  let first: Pixel = [0, 0];
  let gap: Pixel = [0, 0];
  let preview: CapturePreview | null = null;
  /** A spacing or finish request is out: further clicks wait for it. */
  let busy = false;
  let previewing = false;
  let painting = false;

  const line = (x1: number, y1: number, x2: number, y2: number) => {
    g.beginPath();
    g.moveTo(x1, y1);
    g.lineTo(x2, y2);
    g.stroke();
  };

  const paint = () => {
    painting = false;
    g.clearRect(0, 0, width, height);
    g.drawImage(picture, 0, 0);
    g.fillStyle = DIM;
    g.fillRect(0, 0, width, height);
    if (clickTimes < 2 && cursor) {
      const [cx, cy] = cursor;
      g.strokeStyle = CROSSHAIR;
      g.lineWidth = 2;
      line(0, cy, width, cy);
      line(cx, 0, cx, height);
      // Step 2: five lines each way from the origin toward the pointer's quadrant.
      const [x1, y1] = origin;
      const sx = Math.sign(cx - x1);
      const sy = Math.sign(cy - y1);
      if (clickTimes === 1 && sx !== 0 && sy !== 0) {
        const gapW = Math.abs(x1 - cx);
        const gapH = Math.abs(y1 - cy);
        g.strokeStyle = GUIDE;
        g.lineWidth = 4;
        for (let i = 0; i < 5; i++) {
          line(x1 + sx * gapW * i, y1, x1 + sx * gapW * i, y1 + sy * gapH * 5);
          line(x1, y1 + sy * gapH * i, x1 + sx * gapW * 5, y1 + sy * gapH * i);
        }
      }
    }
    if (clickTimes === 2 && preview) {
      const p = preview;
      g.strokeStyle = AREA;
      g.lineWidth = 3;
      g.strokeRect(p.x_min, p.y_min, p.width, p.height);
      for (let i = 1; i < p.columns; i++) {
        const x = Math.trunc(p.x_min + p.cell_width * i);
        line(x, p.y_min, x, p.y_min + p.height);
      }
      for (let j = 1; j < p.rows; j++) {
        const y = Math.trunc(p.y_min + p.cell_height * j);
        line(p.x_min, y, p.x_min + p.width, y);
      }
      const radius = Math.trunc(Math.min(p.cell_width, p.cell_height) / 3.5);
      for (let r = 0; r <= p.rows; r++) {
        for (let c = 0; c <= p.columns; c++) {
          const stone = p.stones[r * (p.columns + 1) + c];
          if (stone !== 1 && stone !== 2) continue;
          // fillOval(cx - r, cy - r, 2r + 1, 2r + 1): yellow for black, magenta for white.
          g.fillStyle = stone === 1 ? "rgb(255, 255, 0)" : "rgb(255, 0, 255)";
          g.beginPath();
          g.arc(Math.round(p.x_min + c * p.cell_width) + 0.5, Math.round(p.y_min + r * p.cell_height) + 0.5, radius + 0.5, 0, Math.PI * 2);
          g.fill();
        }
      }
    }
  };
  const repaint = () => {
    if (painting) return;
    painting = true;
    requestAnimationFrame(paint);
  };

  /** One preview request at a time; the latest pointer position wins. */
  const requestPreview = async () => {
    if (previewing || !cursor) return;
    previewing = true;
    const at = cursor;
    try {
      const result = await invoke<CapturePreview | null>("tsumego_capture_preview", { origin, gap, cursor: at });
      if (clickTimes === 2) preview = result;
    } finally {
      previewing = false;
    }
    repaint();
    if (cursor !== at) void requestPreview();
  };

  const pixelAt = (e: MouseEvent): Pixel => [
    Math.round((e.clientX * width) / window.innerWidth),
    Math.round((e.clientY * height) / window.innerHeight),
  ];

  window.addEventListener("mousemove", (e) => {
    cursor = pixelAt(e);
    if (clickTimes === 2) void requestPreview();
    else repaint();
  });

  window.addEventListener("click", async (e) => {
    if (e.button !== 0 || busy) return;
    const p = pixelAt(e);
    cursor = p;
    if (clickTimes === 0) {
      origin = p;
      clickTimes = 1;
      repaint();
    } else if (clickTimes === 1) {
      busy = true;
      // null: no spacing; Rust has already cancelled and is closing this window.
      const spacing = await invoke<Pixel | null>("tsumego_capture_spacing", { origin, at: p });
      if (!spacing) return;
      first = p;
      gap = spacing;
      clickTimes = 2;
      busy = false;
      repaint();
    } else {
      busy = true;
      await invoke("tsumego_capture_finish", { origin, first, cursor: p });
    }
  });
  repaint();
}
