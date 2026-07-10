/** Human byte size, binary units, one decimal ("4.2 MiB"). */
export function formatBytes(bytes: number): string {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${bytes} B` : `${value.toFixed(1)} ${units[unit]}`;
}

/** Saved percentage as "63%", or null when nothing was gained. */
export function formatSaving(inputBytes: number, outputBytes: number): string | null {
  if (inputBytes <= 0 || outputBytes >= inputBytes) return null;
  return `${Math.round(((inputBytes - outputBytes) / inputBytes) * 100)}%`;
}
