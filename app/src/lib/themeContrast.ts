export type Rgb = readonly [number, number, number];

function linearChannel(value: number): number {
  const channel = value / 255;
  return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
}

export function relativeLuminance(color: Rgb): number {
  return (
    0.2126 * linearChannel(color[0]) +
    0.7152 * linearChannel(color[1]) +
    0.0722 * linearChannel(color[2])
  );
}

export function contrastRatio(first: Rgb, second: Rgb): number {
  const firstLuminance = relativeLuminance(first);
  const secondLuminance = relativeLuminance(second);
  return (
    (Math.max(firstLuminance, secondLuminance) + 0.05) /
    (Math.min(firstLuminance, secondLuminance) + 0.05)
  );
}

export function compositeColor(foreground: Rgb, alpha: number, background: Rgb): Rgb {
  const mix = (index: 0 | 1 | 2): number =>
    Math.round(foreground[index] * alpha + background[index] * (1 - alpha));
  return [mix(0), mix(1), mix(2)];
}
