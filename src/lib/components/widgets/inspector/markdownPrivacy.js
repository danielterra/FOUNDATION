import { marked } from 'marked';

const RASTER_DATA_IMAGE = /^data:image\/(?:png|jpe?g|gif|webp|avif);base64,[a-z0-9+/]+={0,2}$/i;

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, character => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
  })[character]);
}

function decodeBasicEntities(value) {
  return value.replace(/&(?:amp|lt|gt|quot|apos|#39|#x27|#\d+|#x[\da-f]+);/gi, entity => {
    const named = { '&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&apos;': "'", '&#39;': "'", '&#x27;': "'" };
    const normalized = entity.toLowerCase();
    if (named[normalized] !== undefined) return named[normalized];
    const numeric = normalized.startsWith('&#x')
      ? Number.parseInt(normalized.slice(3, -1), 16)
      : Number.parseInt(normalized.slice(2, -1), 10);
    return Number.isFinite(numeric) && numeric > 0 && numeric <= 0x10ffff
      ? String.fromCodePoint(numeric)
      : entity;
  });
}

function readTagEnd(html, start) {
  let quote = null;
  for (let index = start; index < html.length; index += 1) {
    const character = html[index];
    if (quote) {
      if (character === quote) quote = null;
    } else if (character === '"' || character === "'") {
      quote = character;
    } else if (character === '>') {
      return index + 1;
    }
  }
  return html.length;
}

function readImageAttributes(tag) {
  const attributes = {};
  let index = 4;
  while (index < tag.length) {
    while (/\s|\//.test(tag[index] ?? '')) index += 1;
    const nameStart = index;
    while (index < tag.length && !/[\s=/>]/.test(tag[index])) index += 1;
    if (nameStart === index) break;
    const name = tag.slice(nameStart, index).toLowerCase();
    while (/\s/.test(tag[index] ?? '')) index += 1;
    if (tag[index] !== '=') continue;
    index += 1;
    while (/\s/.test(tag[index] ?? '')) index += 1;
    const quote = tag[index] === '"' || tag[index] === "'" ? tag[index++] : null;
    const valueStart = index;
    if (quote) {
      while (index < tag.length && tag[index] !== quote) index += 1;
      attributes[name] = tag.slice(valueStart, index);
      index += 1;
    } else {
      while (index < tag.length && !/[\s>]/.test(tag[index])) index += 1;
      attributes[name] = tag.slice(valueStart, index);
    }
  }
  return attributes;
}

function blockedImageMarkup(alt) {
  const label = decodeBasicEntities(alt.trim());
  const text = label ? `Imagem externa bloqueada: ${label}` : 'Imagem externa bloqueada';
  return `<span class="external-image-blocked">${escapeHtml(text)}</span>`;
}

function imageMarkup(href, alt, title) {
  const source = String(href ?? '').trim();
  if (/^https?:/i.test(source) || source.startsWith('//') || source.includes('&')) return blockedImageMarkup(alt);
  if (/^data:/i.test(source) && !RASTER_DATA_IMAGE.test(source)) return escapeHtml(alt || 'Imagem não exibida');
  const attributes = [`src="${escapeHtml(source)}"`, `alt="${escapeHtml(alt ?? '')}"`];
  if (title) attributes.push(`title="${escapeHtml(title)}"`);
  return `<img ${attributes.join(' ')}>`;
}

export function replaceRemoteImageTags(html) {
  let output = '';
  let cursor = 0;
  while (cursor < html.length) {
    const open = html.indexOf('<', cursor);
    if (open < 0) break;
    output += html.slice(cursor, open);
    if (html.startsWith('<!--', open)) {
      const close = html.indexOf('-->', open + 4);
      if (close < 0) {
        output += html.slice(open);
        cursor = html.length;
        break;
      }
      output += html.slice(open, close + 3);
      cursor = close + 3;
      continue;
    }
    const end = readTagEnd(html, open + 1);
    const tag = html.slice(open, end);
    if (/^<img(?:\s|\/?>)/i.test(tag)) {
      const source = readImageAttributes(tag).src ?? '';
      const alt = readImageAttributes(tag).alt ?? '';
      const decodedSource = decodeBasicEntities(source).trim();
      if (/^https?:/i.test(decodedSource) || decodedSource.startsWith('//') || source.includes('&')) {
        output += blockedImageMarkup(alt);
      } else {
        output += imageMarkup(decodedSource, decodeBasicEntities(alt), readImageAttributes(tag).title);
      }
    } else {
      output += tag;
    }
    cursor = end;
  }
  return output + html.slice(cursor);
}

export function parseMarkdown(text) {
  const renderer = new marked.Renderer();
  renderer.image = ({ href, title, text: alt }) => imageMarkup(href, alt, title);
  renderer.html = ({ text: html }) => replaceRemoteImageTags(html);
  return marked.parse(text ?? '', { renderer });
}
