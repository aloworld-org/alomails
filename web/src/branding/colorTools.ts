interface Rgb { r: number; g: number; b: number }

export function contrastRatio(first: string, second: string): number {
  const a = luminance(hexToRgb(first));
  const b = luminance(hexToRgb(second));
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

export function readableInk(background: string): "#FFFFFF" | "#102A43" {
  return contrastRatio(background, "#FFFFFF") >= contrastRatio(background, "#102A43")
    ? "#FFFFFF"
    : "#102A43";
}

export function toneScale(color: string): string[] {
  return [
    mix(color, "#FFFFFF", 0.88),
    mix(color, "#FFFFFF", 0.66),
    mix(color, "#FFFFFF", 0.36),
    color,
    mix(color, "#000000", 0.18),
    mix(color, "#000000", 0.36),
  ];
}

export function contrastPasses(color: string, ink: string): boolean {
  return contrastRatio(color, ink) >= 4.5;
}

function hexToRgb(value: string): Rgb {
  const safe = /^#[0-9A-F]{6}$/i.test(value) ? value.slice(1) : "000000";
  return {
    r: Number.parseInt(safe.slice(0, 2), 16),
    g: Number.parseInt(safe.slice(2, 4), 16),
    b: Number.parseInt(safe.slice(4, 6), 16),
  };
}

function mix(color: string, target: string, amount: number): string {
  const from = hexToRgb(color);
  const to = hexToRgb(target);
  const channel = (start: number, end: number) =>
    Math.round(start + (end - start) * amount).toString(16).padStart(2, "0");
  return `#${channel(from.r, to.r)}${channel(from.g, to.g)}${channel(from.b, to.b)}`.toUpperCase();
}

function luminance({ r, g, b }: Rgb): number {
  const channel = (value: number) => {
    const normalized = value / 255;
    return normalized <= 0.03928
      ? normalized / 12.92
      : ((normalized + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}
