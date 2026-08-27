export const IMAGE_FRAME = {
  natural: "",
  landscape: "aspect-[16/7]",
  square: "aspect-square",
} as const;

export const IMAGE_BLOCK_ZOOM = {
  50: "scale-50",
  75: "scale-75",
  100: "scale-100",
  125: "scale-125",
  150: "scale-150",
  175: "scale-[1.75]",
  200: "scale-200",
} as const;

export const IMAGE_COLUMN_GRID = {
  "33-67": { left: "md:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]", right: "md:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]" },
  "40-60": { left: "md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]", right: "md:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]" },
  "50-50": { left: "md:grid-cols-2", right: "md:grid-cols-2" },
  "60-40": { left: "md:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]", right: "md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]" },
  "67-33": { left: "md:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]", right: "md:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]" },
} as const;
