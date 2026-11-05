import { convertFileSrc } from '@tauri-apps/api/core';

const RASTER_DATA_IMAGE = /^data:image\/(?:png|jpe?g|gif|webp|avif);base64,[a-z0-9+/]+={0,2}$/i;

export function inspectorIconSource(icon: string | null | undefined): string | null {
  if (!icon || icon.length > 1_500_000) return null;
  const source = icon.trim();
  if (/^https?:/i.test(source) || source.startsWith('//')) return null;
  if (/^data:/i.test(source)) return RASTER_DATA_IMAGE.test(source) ? source : null;
  if (/^file:\/\//i.test(source)) {
    try {
      const url = new URL(source);
      if (url.search || url.hash) return null;
      return convertFileSrc(decodeURIComponent(url.pathname));
    } catch {
      return null;
    }
  }
  if ((source.startsWith('/') && !source.startsWith('//')) || /^[a-z]:[\\/]/i.test(source)) {
    try {
      return convertFileSrc(source);
    } catch {
      return null;
    }
  }
  return null;
}

export function inspectorIconSymbol(icon: string | null | undefined): string {
  const symbol = icon?.trim();
  if (!symbol || /^[a-z][a-z0-9+.-]*:/i.test(symbol) || symbol.startsWith('/')) return 'image';
  return symbol;
}
