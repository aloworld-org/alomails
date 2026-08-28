// A picture chosen for a quotation, made fit to travel with the design.
//
// The design is saved with the quote and printed into its PDF, so a picture
// is stored as what a page can use — not the 12 MB a phone produces. Anything
// larger than a print needs is scaled down here, in the browser, before it is
// stored: a JPEG stays a JPEG, a PNG stays a PNG (its transparency with it),
// and anything else becomes a JPEG on white.

/** The longest side a stored picture keeps, in pixels — an A4 column at
 *  print resolution, and nothing on the page is wider than the column. */
export const QUOTE_IMAGE_MAX_SIDE = 1600;

/** The size a picture is stored at: its own, or scaled down to the cap with
 *  its proportions kept. */
export function quoteImageTarget(
  width: number,
  height: number,
  maxSide = QUOTE_IMAGE_MAX_SIDE,
): { width: number; height: number } {
  const longest = Math.max(width, height, 1);
  const scale = Math.min(1, maxSide / longest);
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}

function shrink(image: HTMLImageElement, original: string, type: string): string {
  const target = quoteImageTarget(image.naturalWidth, image.naturalHeight);
  const keepPng = type === "image/png";
  const unchanged =
    target.width === image.naturalWidth &&
    target.height === image.naturalHeight;
  if (unchanged && (type === "image/jpeg" || keepPng)) return original;
  const canvas = document.createElement("canvas");
  canvas.width = target.width;
  canvas.height = target.height;
  const context = canvas.getContext("2d");
  if (context === null) return original;
  if (!keepPng) {
    // A JPEG has no transparency: whatever was see-through is paper-white.
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, target.width, target.height);
  }
  context.drawImage(image, 0, 0, target.width, target.height);
  try {
    return keepPng
      ? canvas.toDataURL("image/png")
      : canvas.toDataURL("image/jpeg", 0.88);
  } catch {
    return original;
  }
}

/** Reads a chosen file and hands back the data URL to store. */
export function readQuoteImage(file: File, done: (value: string) => void) {
  const reader = new FileReader();
  reader.onload = () => {
    if (typeof reader.result !== "string") return;
    const original = reader.result;
    const image = new Image();
    image.onload = () => done(shrink(image, original, file.type));
    // Not decodable as a picture: stored as chosen, and the print shows its
    // caption in its place.
    image.onerror = () => done(original);
    image.src = original;
  };
  reader.readAsDataURL(file);
}
