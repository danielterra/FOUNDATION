import { convertFileSrc } from '@tauri-apps/api/core';

/**
 * True when `icon` points at an image (web URL, data URI, or a local file)
 * rather than naming a Material Symbols ligature such as `person` or `star`.
 *
 * Anything that is NOT an icon URL must be rendered as a ligature; rendering an
 * image reference as a ligature is what makes raw `file:///…` paths show up as
 * text in the UI.
 */
export function isIconUrl(icon: string | null | undefined): boolean {
  if (!icon) return false;
  return (
    icon.startsWith('http://') ||
    icon.startsWith('https://') ||
    icon.startsWith('data:') ||
    icon.startsWith('file://') ||
    icon.startsWith('/')
  );
}

/**
 * Turn an image icon into a value an `<img src>` can actually load.
 *
 * Local files — `file:///…` or a bare absolute path — go through Tauri's asset
 * protocol: the webview's CSP blocks a raw `file://` URL, so an unconverted
 * path renders as a broken image even when the branch is right.
 *
 * Returns null for ligature names, so callers can use it as the branch test too.
 */
export function resolveIconSrc(icon: string | null | undefined): string | null {
  if (!icon) return null;
  if (icon.startsWith('http://') || icon.startsWith('https://') || icon.startsWith('data:')) {
    return icon;
  }
  if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''));
  if (icon.startsWith('/')) return convertFileSrc(icon);
  return null;
}
